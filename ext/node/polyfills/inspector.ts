// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.
// Copyright Joyent and Node contributors. All rights reserved. MIT license.

import {
  op_inspector_get_notification,
  op_inspector_post,
  op_void_async_deferred,
} from "ext:core/ops";
import { EventEmitter } from "node:events";
import { notImplemented } from "ext:deno_node/_utils.ts";
import { primordials } from "ext:core/mod.js";

const {
  SafeMap,
} = primordials;

class Session extends EventEmitter {
  #connection = null;
  #nextId = 1;
  #messageCallbacks = new SafeMap();

  /** Connects the session to the inspector back-end. */
  connect(): void {
    // notImplemented("inspector.Session.prototype.connect");
    (async () => {
      while (true) {
        let notification;
        try {
          notification = await op_inspector_get_notification();
        } catch (_e) {
          break;
        }
        this.emit(notification.method, notification);
        this.emit("inspectorNotification", notification);
      }
    })();
  }

  /** Connects the session to the main thread
   * inspector back-end. */
  connectToMainThread(): void {
    notImplemented("inspector.Session.prototype.connectToMainThread");
  }

  /** Posts a message to the inspector back-end. */
  post(
    method: string,
    params?: Record<string, unknown>,
    callback?: (...args: unknown[]) => void,
  ): void {
    Promise.all([
      op_inspector_post(method, params).then((
        response,
      ) => callback?.(response)),
      op_void_async_deferred(),
    ]);
  }

  /** Immediately closes the session, all pending
   * message callbacks will be called with an
   * error.
   */
  disconnect() {
    notImplemented("inspector.Session.prototype.disconnect");
  }
}

/** Activates inspector on host and port.
 * See https://nodejs.org/api/inspector.html#inspectoropenport-host-wait */
function open(_port?: number, _host?: string, _wait?: boolean) {
  notImplemented("inspector.Session.prototype.open");
}

/** Deactivate the inspector. Blocks until there are no active connections.
 * See https://nodejs.org/api/inspector.html#inspectorclose */
function close() {
  notImplemented("inspector.Session.prototype.close");
}

/** Return the URL of the active inspector, or undefined if there is none.
 * See https://nodejs.org/api/inspector.html#inspectorurl */
function url() {
  // TODO(kt3k): returns undefined for now, which means the inspector is not activated.
  return undefined;
}

/** Blocks until a client (existing or connected later) has sent Runtime.runIfWaitingForDebugger command.
 * See https://nodejs.org/api/inspector.html#inspectorwaitfordebugger */
function waitForDebugger() {
  notImplemented("inspector.wairForDebugger");
}

const console = globalThis.console;

export { close, console, open, Session, url, waitForDebugger };

export default {
  close,
  console,
  open,
  Session,
  url,
  waitForDebugger,
};
