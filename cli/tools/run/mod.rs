// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.

use std::borrow::Cow;
use std::io::Read;

use deno_config::workspace::PackageJsonDepResolution;
use deno_core::anyhow::Context;
use deno_core::error::AnyError;
use deno_core::futures::io::BufReader;
use deno_core::futures::stream::FuturesOrdered;
use deno_core::futures::StreamExt;
use deno_core::unsync::spawn;
use deno_runtime::deno_permissions::Permissions;
use deno_runtime::deno_permissions::PermissionsContainer;
use deno_runtime::ops::otel::OtelConfig;
use deno_runtime::WorkerExecutionMode;
use eszip::EszipV2;
use tokio_util::compat::TokioAsyncReadCompatExt;

use crate::args::CaData;
use crate::args::EvalFlags;
use crate::args::Flags;
use crate::args::RunFlags;
use crate::args::WatchFlagsWithPaths;
use crate::factory::CliFactory;
use crate::factory::CliFactoryBuilder;
use crate::file_fetcher::File;
use crate::standalone::binary::Metadata;
use crate::standalone::binary::SerializedWorkspaceResolver;
use crate::util;
use crate::util::file_watcher::WatcherRestartMode;

pub mod hmr;

pub async fn run_script(
  mode: WorkerExecutionMode,
  flags: Flags,
  watch: Option<WatchFlagsWithPaths>,
) -> Result<i32, AnyError> {
  if !flags.has_permission() && flags.has_permission_in_argv() {
    log::warn!(
      "{}",
      crate::colors::yellow(
        r#"Permission flags have likely been incorrectly set after the script argument.
To grant permissions, set them before the script argument. For example:
    deno run --allow-read=. main.js"#
      )
    );
  }

  if let Some(watch_flags) = watch {
    return run_with_watch(mode, flags, watch_flags).await;
  }

  // TODO(bartlomieju): actually I think it will also fail if there's an import
  // map specified and bare specifier is used on the command line
  let factory = CliFactory::from_flags(flags.clone())?;
  let deno_dir = factory.deno_dir()?;
  let http_client = factory.http_client_provider();
  let cli_options = factory.cli_options();
  let permissions = PermissionsContainer::new(Permissions::from_options(
    &cli_options.permissions_options()?,
  )?);
  let main_module = cli_options.resolve_main_module()?;

  // Run a background task that checks for available upgrades or output
  // if an earlier run of this background task found a new version of Deno.
  #[cfg(feature = "upgrade")]
  super::upgrade::check_for_upgrades(
    http_client.clone(),
    deno_dir.upgrade_check_file_path(),
  );

  if cli_options.unstable_sloppy_imports() {
    log::warn!(
      "{} Sloppy imports are not recommended and have a negative impact on performance.",
      crate::colors::yellow("Warning"),
    );
  }

  maybe_npm_install(&factory).await?;

  let worker_factory = factory.create_cli_main_worker_factory().await?;
  let mut worker = worker_factory
    .create_main_worker(mode, main_module, permissions)
    .await?;

  let exit_code = worker.run().await?;
  Ok(exit_code)
}

pub async fn run_from_stdin(flags: Flags) -> Result<i32, AnyError> {
  let factory = CliFactory::from_flags(flags)?;
  let cli_options = factory.cli_options();
  let main_module = cli_options.resolve_main_module()?;

  maybe_npm_install(&factory).await?;

  let file_fetcher = factory.file_fetcher()?;
  let worker_factory = factory.create_cli_main_worker_factory().await?;
  let permissions = PermissionsContainer::new(Permissions::from_options(
    &cli_options.permissions_options()?,
  )?);
  let mut source = Vec::new();
  std::io::stdin().read_to_end(&mut source)?;
  // Save a fake file into file fetcher cache
  // to allow module access by TS compiler
  file_fetcher.insert_memory_files(File {
    specifier: main_module.clone(),
    maybe_headers: None,
    source: source.into(),
  });

  let mut worker = worker_factory
    .create_main_worker(WorkerExecutionMode::Run, main_module, permissions)
    .await?;
  let exit_code = worker.run().await?;
  Ok(exit_code)
}

// TODO(bartlomieju): this function is not handling `exit_code` set by the runtime
// code properly.
async fn run_with_watch(
  mode: WorkerExecutionMode,
  flags: Flags,
  watch_flags: WatchFlagsWithPaths,
) -> Result<i32, AnyError> {
  util::file_watcher::watch_recv(
    flags,
    util::file_watcher::PrintConfig::new_with_banner(
      if watch_flags.hmr { "HMR" } else { "Watcher" },
      "Process",
      !watch_flags.no_clear_screen,
    ),
    WatcherRestartMode::Automatic,
    move |flags, watcher_communicator, _changed_paths| {
      Ok(async move {
        let factory = CliFactoryBuilder::new()
          .build_from_flags_for_watcher(flags, watcher_communicator.clone())?;
        let cli_options = factory.cli_options();
        let main_module = cli_options.resolve_main_module()?;

        maybe_npm_install(&factory).await?;

        let _ = watcher_communicator.watch_paths(cli_options.watch_paths());

        let permissions = PermissionsContainer::new(Permissions::from_options(
          &cli_options.permissions_options()?,
        )?);
        let mut worker = factory
          .create_cli_main_worker_factory()
          .await?
          .create_main_worker(mode, main_module, permissions)
          .await?;

        if watch_flags.hmr {
          worker.run().await?;
        } else {
          worker.run_for_watcher().await?;
        }

        Ok(())
      })
    },
  )
  .await?;

  Ok(0)
}

pub async fn eval_command(
  flags: Flags,
  eval_flags: EvalFlags,
) -> Result<i32, AnyError> {
  let factory = CliFactory::from_flags(flags)?;
  let cli_options = factory.cli_options();
  let file_fetcher = factory.file_fetcher()?;
  let main_module = cli_options.resolve_main_module()?;

  maybe_npm_install(&factory).await?;

  // Create a dummy source file.
  let source_code = if eval_flags.print {
    format!("console.log({})", eval_flags.code)
  } else {
    eval_flags.code
  };

  // Save a fake file into file fetcher cache
  // to allow module access by TS compiler.
  file_fetcher.insert_memory_files(File {
    specifier: main_module.clone(),
    maybe_headers: None,
    source: source_code.into_bytes().into(),
  });

  let permissions = PermissionsContainer::new(Permissions::from_options(
    &cli_options.permissions_options()?,
  )?);
  let worker_factory = factory.create_cli_main_worker_factory().await?;
  let mut worker = worker_factory
    .create_main_worker(WorkerExecutionMode::Eval, main_module, permissions)
    .await?;
  let exit_code = worker.run().await?;
  Ok(exit_code)
}

async fn maybe_npm_install(factory: &CliFactory) -> Result<(), AnyError> {
  // ensure an "npm install" is done if the user has explicitly
  // opted into using a managed node_modules directory
  if factory.cli_options().node_modules_dir_enablement() == Some(true) {
    if let Some(npm_resolver) = factory.npm_resolver().await?.as_managed() {
      npm_resolver.ensure_top_level_package_json_install().await?;
    }
  }
  Ok(())
}

pub async fn run_eszip(
  flags: Flags,
  run_flags: RunFlags,
) -> Result<i32, AnyError> {
  // TODO(bartlomieju): actually I think it will also fail if there's an import
  // map specified and bare specifier is used on the command line
  let factory = CliFactory::from_flags(flags.clone())?;
  let cli_options = factory.cli_options();

  // entrypoint#path1,path2,...
  let (entrypoint, files) = run_flags
    .script
    .split_once("#")
    .with_context(|| "eszip: invalid script string")?;

  println!("running eszip: entrypoint={} files={}", entrypoint, files);

  // TODO: handle paths that contain ','
  let files = files.split(",").collect::<Vec<_>>();
  let mut headers = FuturesOrdered::new();
  for path in files {
    let file = tokio::fs::File::open(path).await?;
    let eszip = BufReader::new(file.compat());
    let path = path.to_string();

    headers.push_back(async move {
      let (eszip, loader) = match EszipV2::parse(eszip).await {
        Ok(x) => x,
        Err(e) => {
          log::error!("Error parsing eszip header at {}: {}", path, e);
          std::process::exit(1);
        }
      };
      spawn(async move {
        if let Err(e) = loader.await {
          log::error!("Error loading eszip at {}: {}", path, e);
          std::process::exit(1);
        }
      });
      eszip
    });
  }
  let headers = headers.collect::<Vec<_>>().await;

  let ca_data = match cli_options.ca_data() {
    Some(CaData::File(ca_file)) => Some(
      std::fs::read(ca_file).with_context(|| format!("Reading: {ca_file}"))?,
    ),
    Some(CaData::Bytes(bytes)) => Some(bytes.clone()),
    None => None,
  };

  crate::standalone::run(
    headers,
    Metadata {
      argv: flags.argv,
      seed: flags.seed,
      permissions: flags.permissions,
      location: flags.location,
      v8_flags: flags.v8_flags,
      log_level: flags.log_level,
      ca_stores: flags.ca_stores,
      ca_data,
      unsafely_ignore_certificate_errors: flags
        .unsafely_ignore_certificate_errors,
      entrypoint_key: entrypoint.to_string(),
      node_modules: None,
      disable_deprecated_api_warning: false,
      unstable_config: flags.unstable_config,
      env_vars_from_env_file: Default::default(),
      workspace_resolver: SerializedWorkspaceResolver {
        import_map: None,
        package_jsons: Default::default(),
        pkg_json_resolution: PackageJsonDepResolution::Disabled,
      },
      otel_config: flags.otel.then(|| OtelConfig {
        default_service_name: Cow::Borrowed("deno"),
        default_service_version: Cow::Borrowed(crate::version::deno()),
      }),
    },
    run_flags.script.as_bytes(),
    "run-eszip",
  )
  .await
}
