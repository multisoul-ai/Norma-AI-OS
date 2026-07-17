const NATIVE_HOST = "com.norma.os.browser_auth";
const PROTOCOL_VERSION = 1;
const MAX_COOKIES = 512;
const RECONNECT_DELAY_MS = 1_000;

let nativePort;
let reconnectTimer;

function connectNativeHost() {
  clearTimeout(reconnectTimer);
  nativePort = chrome.runtime.connectNative(NATIVE_HOST);
  nativePort.onMessage.addListener(handleNativeRequest);
  nativePort.onDisconnect.addListener(() => {
    nativePort = undefined;
    reconnectTimer = setTimeout(connectNativeHost, RECONNECT_DELAY_MS);
  });
}

async function handleNativeRequest(request) {
  if (
    request?.version !== PROTOCOL_VERSION ||
    request?.type !== "get_cookies" ||
    !Number.isSafeInteger(request.request_id) ||
    !Number.isSafeInteger(request.node_id)
  ) {
    return;
  }

  try {
    const url = new URL(request.url);
    if (url.protocol !== "http:" && url.protocol !== "https:") {
      throw new Error("unsupported_url");
    }
    const cookies = await chrome.cookies.getAll({ url: url.href });
    if (cookies.length > MAX_COOKIES) {
      throw new Error("cookie_limit_exceeded");
    }
    nativePort?.postMessage({
      version: PROTOCOL_VERSION,
      type: "cookie_export",
      request_id: request.request_id,
      node_id: request.node_id,
      url: request.url,
      cookies,
    });
  } catch (error) {
    nativePort?.postMessage({
      version: PROTOCOL_VERSION,
      type: "cookie_export_error",
      request_id: request.request_id,
      node_id: request.node_id,
      url: request.url,
      code: error instanceof Error ? error.message : "cookie_export_failed",
    });
  }
}

connectNativeHost();
