# Browser Chrome Auth Sharing SPEC

> 历史：见 git 提交。原文路径：`docs/design-docs/2026-07-16-browser-chrome-auth.md`、`docs/exec-plans/2026-07-16-browser-chrome-auth.md`、`docs/superpowers/plans/2026-07-05-cef-browser-engine-switch.md`。

设计结论已在本文：只用 Manifest V3 扩展经 Native Messaging 读取当前 URL 的 Cookie；禁止直接复用 Chrome Profile 或解密 Cookie 数据库；Cookie 值不进 SQLite 或界面。执行计划六个切片当时均已勾选。默认引擎仍是 Wry，CEF 只在 `chromium-browser` feature 下编译。验收表里的 `crates/e2e` 是当时证据，该套件后来已移除。

## Goal

Let a user explicitly import the current page's authenticated Google Chrome session into a Norma Browser card without opening, copying, or mutating Chrome's profile directory.

## User behavior

1. A Browser card shows a `Chrome` session button in its toolbar.
2. Clicking the button requests cookies for that card's exact current HTTP(S) URL from the user-enabled, bundled Norma Chrome extension.
3. The extension uses Chrome's `chrome.cookies` API and sends the result through Chrome Native Messaging to the running Norma process.
4. Norma imports compatible cookies into that Browser card backend, reloads the page, and shows `Ready` when the import succeeds.
5. If the extension/native host is unavailable, the card remains usable and shows a recoverable error with the bundled extension path; navigation, selection, address editing, and other cards keep working.

## Security and privacy contract

- User activation is required for every import. There is no silent startup export.
- A request is scoped to one exact HTTP(S) URL. `file:`, custom, and invalid URLs are rejected.
- Norma does not serialize Cookie values into its SQLite stores, app-owned files, tracing, panic text, or UI text. After import, the isolated Wry/CEF website data store may persist a persistent Cookie until its original expiry, matching normal embedded-browser behavior.
- Chrome's profile directory, Cookie database, Local State, encryption keys, and Keychain items are never read or copied.
- The Native Messaging host accepts only the bundled extension's fixed origin. The local Unix socket and parent directory are user-only.
- Partitioned cookies are skipped because Wry's public cookie API cannot preserve their partition key. Importing them as unpartitioned cookies would widen their scope.
- Expired or structurally invalid cookies are rejected. Import limits bound message size and cookie count.

## Compatibility boundary

The v1 shared authorization surface is HTTP cookies, including HttpOnly cookies exposed by the official Chrome extension API. It does not claim to transfer local/session storage, IndexedDB, passkeys, client certificates, Chrome account tokens, or browser extensions. Sites relying on those mechanisms can still require login.

## Acceptance criteria and evidence

| Criterion | Automated evidence |
|---|---|
| Protocol accepts a valid URL-scoped Chrome cookie export and preserves supported attributes | `app` unit tests over the public auth protocol/conversion seam |
| Invalid URLs, expired cookies, oversized exports, and partitioned cookies are rejected/skipped without exposing values | `app` unit tests with exact positive and negative assertions |
| Native Messaging uses length-prefixed JSON and rejects malformed/oversized frames | helper binary/unit protocol tests |
| Wry and CEF backends expose cookie import without changing existing navigation/focus lifecycle | focused backend tests plus `cargo test -p app` |
| Wry import does not synchronously wait for WebKit Cookie completion on the GPUI thread | performance guardrail plus async completion Browser event |
| Browser toolbar offers explicit Chrome import and displays pending/success/failure states | `crates/e2e` L1 fake-boundary flow |
| Missing connector does not prevent Browser creation, address edit, navigation, or selection | existing Browser e2e suite plus new failure-path e2e |
| Default app build remains independent of CEF resources | `cargo check -p app` |
| Chromium feature still compiles | `cargo check -p app --features chromium-browser` |
| No repository constraints regress | guardrail scripts and workspace tests |

## Non-goals

- Direct Chrome profile reuse or cloning.
- Remote-debugging access to Chrome's default data directory.
- Background two-way synchronization of all browser state.
- Installing a Chrome extension without Chrome's user-visible consent.
- Publishing the extension to Chrome Web Store; development integration uses Chrome's explicit **Load unpacked** flow.
