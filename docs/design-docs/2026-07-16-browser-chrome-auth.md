# Browser Chrome Auth Sharing Design

## Decision

Use a Manifest V3 Chrome extension plus a bundled Native Messaging helper. The extension is the only component allowed to read Chrome cookies. Norma requests only the current Browser card URL and imports the returned cookies into the existing Wry/CEF backend boundary.

Direct profile reuse is rejected because Chrome owns the profile with a process singleton, Chromium-family products cannot safely write the same profile concurrently, and Chrome 136+ ignores remote-debugging switches for its default data directory. Direct database decryption is also rejected because it bypasses Chrome's permission UI and couples Norma to encrypted private storage formats.

## Components

```text
Browser toolbar action
        |
        v
BrowserStore auth state ---- request channel ----> app-local broker thread
        ^                                             |
        |                                             v
NativeBrowserView <---- validated cookies ---- Unix socket (0600)
                                                      |
                                                      v
                                      norma-browser-auth-host
                                                      |
                                                      v
                                      Chrome Native Messaging
                                                      |
                                                      v
                                       MV3 chrome.cookies API
```

### Protocol module

Owns versioned request/response JSON, URL scope validation, size/count limits, cookie attribute conversion, and redacted errors. It contains no GPUI or native browser code.

### Native host

`norma-browser-auth-host` is a small stdio-to-Unix-socket proxy. It implements Chrome's four-byte native-endian length-prefixed JSON framing and never interprets or logs Cookie values. Its registered host manifest contains one exact `allowed_origins` entry.

### Broker

The app-local broker binds under Norma's Application Support directory with user-only permissions. All accept/read/write waits run on a dedicated thread. Browser UI sends nonblocking channel commands; results return through the existing Browser event queue.

### Browser backends

`NativeBrowserView::import_cookies` delegates to Wry or CEF. Wry schedules one batch through `WKHTTPCookieStore::setCookies` and reports its completion through the existing Browser event queue; it never synchronously waits on the GPUI thread. CEF uses the global request-context CookieManager. Imports happen only after protocol validation; a successful non-empty import reloads the page.

### UI state

Each Browser node has one ephemeral state: `Idle`, `Waiting`, `Ready`, or `Failed`. Import/skip counts are tracing metadata only; the toolbar never renders Cookie names or values. Removing a card removes its state. No auth UI state is persisted with the canvas.

## Failure behavior

- Extension unavailable: request times out, state becomes `Failed`, native view is untouched, and the error banner points to the bundled unpacked-extension directory.
- Malformed response: reject the whole response with a redacted error.
- Individual incompatible cookie: skip it and report only counts.
- All cookies skipped: do not reload; report a recoverable failure.
- Card removed or navigated while response/completion is in flight: discard the stale event without reloading the new page.
- Backend import failure: preserve existing store/page and report only the backend error category.
