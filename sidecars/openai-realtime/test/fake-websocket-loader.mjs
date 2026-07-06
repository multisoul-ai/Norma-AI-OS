import { EventEmitter } from "node:events";

class FakeWebSocket extends EventEmitter {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;

  constructor(url, options = {}) {
    super();
    this.url = url;
    this.options = options;
    this.readyState = FakeWebSocket.CONNECTING;
    this.closedByClient = false;

    process.send?.({
      kind: "ws-created",
      url,
      headers: options.headers ?? {},
    });

    process.on("message", (message) => {
      this.#handleControl(message);
    });
  }

  send(data, callback) {
    const payload = typeof data === "string" ? data : data.toString("utf8");
    process.send?.({
      kind: "ws-send",
      data: payload,
    });
    callback?.();
  }

  close(code = 1000, reason = "") {
    this.closedByClient = true;
    this.readyState = FakeWebSocket.CLOSED;
    process.send?.({
      kind: "ws-close",
      code,
      reason,
    });
    queueMicrotask(() => {
      this.emit("close", code, Buffer.from(reason));
    });
  }

  #handleControl(message) {
    if (!message || typeof message !== "object") {
      return;
    }

    if (message.kind === "ws-open") {
      this.readyState = FakeWebSocket.OPEN;
      this.emit("open");
      return;
    }

    if (message.kind === "ws-message") {
      this.emit("message", JSON.stringify(message.data), false);
      return;
    }

    if (message.kind === "ws-error") {
      this.emit("error", new Error(message.message));
      return;
    }

    if (message.kind === "ws-close-from-server") {
      this.readyState = FakeWebSocket.CLOSED;
      this.emit("close", message.code ?? 1000, Buffer.from(message.reason ?? ""));
    }
  }
}

globalThis.__NORMA_TEST_WEBSOCKET__ = FakeWebSocket;
