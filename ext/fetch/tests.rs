// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.

use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::BodyExt;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

use super::create_http_client;
use super::CreateHttpClientOptions;

static EXAMPLE_CRT: &[u8] = include_bytes!("../tls/testdata/example1_cert.der");
static EXAMPLE_KEY: &[u8] =
  include_bytes!("../tls/testdata/example1_prikey.der");

#[tokio::test]
async fn test_https_proxy_http11() {
  let src_addr = create_https_server(false).await;
  let prx_addr = create_http_proxy(src_addr).await;
  run_test_client(prx_addr, src_addr, false, http::Version::HTTP_11).await;
}

#[tokio::test]
async fn test_https_proxy_h2() {
  let src_addr = create_https_server(true).await;
  let prx_addr = create_http_proxy(src_addr).await;
  run_test_client(prx_addr, src_addr, false, http::Version::HTTP_2).await;
}

#[tokio::test]
async fn test_https_proxy_https_h2() {
  let src_addr = create_https_server(true).await;
  let prx_addr = create_https_proxy(src_addr).await;
  run_test_client(prx_addr, src_addr, true, http::Version::HTTP_2).await;
}

async fn run_test_client(
  prx_addr: SocketAddr,
  src_addr: SocketAddr,
  https: bool,
  ver: http::Version,
) {
  let client = create_http_client(
    "fetch/test",
    CreateHttpClientOptions {
      root_cert_store: None,
      ca_certs: vec![],
      proxy: Some(deno_tls::Proxy {
        url: format!("http{}://{}", if https { "s" } else { "" }, prx_addr),
        basic_auth: None,
      }),
      unsafely_ignore_certificate_errors: Some(vec![]),
      client_cert_chain_and_key: None,
      pool_max_idle_per_host: None,
      pool_idle_timeout: None,
      http1: true,
      http2: true,
    },
  )
  .unwrap();

  let req = http::Request::builder()
    .uri(format!("https://{}/foo", src_addr))
    .body(
      http_body_util::Empty::new()
        .map_err(|err| match err {})
        .boxed(),
    )
    .unwrap();
  let resp = client.send(req).await.unwrap();
  assert_eq!(resp.status(), http::StatusCode::OK);
  assert_eq!(resp.version(), ver);
  let hello = resp.collect().await.unwrap().to_bytes();
  assert_eq!(hello, "hello from server");
}

async fn create_https_server(allow_h2: bool) -> SocketAddr {
  let mut tls_config = deno_tls::rustls::server::ServerConfig::builder()
    .with_no_client_auth()
    .with_single_cert(
      vec![EXAMPLE_CRT.into()],
      webpki::types::PrivateKeyDer::try_from(EXAMPLE_KEY).unwrap(),
    )
    .unwrap();
  if allow_h2 {
    tls_config.alpn_protocols.push("h2".into());
  }
  tls_config.alpn_protocols.push("http/1.1".into());
  let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::from(tls_config));
  let src_tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let src_addr = src_tcp.local_addr().unwrap();

  tokio::spawn(async move {
    while let Ok((sock, _)) = src_tcp.accept().await {
      let conn = tls_acceptor.accept(sock).await.unwrap();
      if conn.get_ref().1.alpn_protocol() == Some(b"h2") {
        let fut = hyper::server::conn::http2::Builder::new(
          hyper_util::rt::TokioExecutor::new(),
        )
        .serve_connection(
          hyper_util::rt::TokioIo::new(conn),
          hyper::service::service_fn(|_req| async {
            Ok::<_, std::convert::Infallible>(http::Response::new(
              http_body_util::Full::<Bytes>::new("hello from server".into()),
            ))
          }),
        );
        tokio::spawn(fut);
      } else {
        let fut = hyper::server::conn::http1::Builder::new().serve_connection(
          hyper_util::rt::TokioIo::new(conn),
          hyper::service::service_fn(|_req| async {
            Ok::<_, std::convert::Infallible>(http::Response::new(
              http_body_util::Full::<Bytes>::new("hello from server".into()),
            ))
          }),
        );
        tokio::spawn(fut);
      }
    }
  });

  src_addr
}

async fn create_http_proxy(src_addr: SocketAddr) -> SocketAddr {
  let prx_tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let prx_addr = prx_tcp.local_addr().unwrap();

  tokio::spawn(async move {
    while let Ok((mut sock, _)) = prx_tcp.accept().await {
      let fut = async move {
        let mut buf = [0u8; 4096];
        let _n = sock.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..7], b"CONNECT");
        let mut dst_tcp =
          tokio::net::TcpStream::connect(src_addr).await.unwrap();
        sock.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.unwrap();
        tokio::io::copy_bidirectional(&mut sock, &mut dst_tcp)
          .await
          .unwrap();
      };
      tokio::spawn(fut);
    }
  });

  prx_addr
}

async fn create_https_proxy(src_addr: SocketAddr) -> SocketAddr {
  let mut tls_config = deno_tls::rustls::server::ServerConfig::builder()
    .with_no_client_auth()
    .with_single_cert(
      vec![EXAMPLE_CRT.into()],
      webpki::types::PrivateKeyDer::try_from(EXAMPLE_KEY).unwrap(),
    )
    .unwrap();
  // Set ALPN, to check our proxy connector. But we shouldn't receive anything.
  tls_config.alpn_protocols.push("h2".into());
  tls_config.alpn_protocols.push("http/1.1".into());
  let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::from(tls_config));
  let prx_tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let prx_addr = prx_tcp.local_addr().unwrap();

  tokio::spawn(async move {
    while let Ok((sock, _)) = prx_tcp.accept().await {
      let mut sock = tls_acceptor.accept(sock).await.unwrap();
      assert_eq!(sock.get_ref().1.alpn_protocol(), None);

      let fut = async move {
        let mut buf = [0u8; 4096];
        let _n = sock.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..7], b"CONNECT");
        let mut dst_tcp =
          tokio::net::TcpStream::connect(src_addr).await.unwrap();
        sock.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.unwrap();
        tokio::io::copy_bidirectional(&mut sock, &mut dst_tcp)
          .await
          .unwrap();
      };
      tokio::spawn(fut);
    }
  });

  prx_addr
}
