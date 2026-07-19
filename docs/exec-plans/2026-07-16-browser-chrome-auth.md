# Browser Chrome Auth Sharing Execution Plan

## Test seams

The agreed public seams for this implementation are:

1. Versioned auth protocol and cookie validation/conversion.
2. Chrome Native Messaging framed stdio helper.
3. `NativeBrowserView` cookie-import lifecycle boundary.
4. Browser toolbar action and observable per-card status through the existing e2e harness, with only the external Chrome boundary faked.

## Vertical slices

- [x] Slice 1: failing protocol tests -> request/response and cookie validation.
- [x] Slice 2: failing framing tests -> native host proxy and registration manifest generation.
- [x] Slice 3: failing backend/guardrail test -> asynchronous Wry import, then CEF parity.
- [x] Slice 4: failing app/e2e test -> toolbar action, broker request, pending/success/failure status.
- [x] Slice 5: extension assets and install/discovery flow.
- [x] Slice 6: focused tests, Browser e2e, app checks, workspace tests, release build, and guardrails.

Verification note: all Browser/auth tests, full e2e, default/CEF checks, release build, and repository guardrails pass. `cargo test -p app` / `cargo test --workspace` still report four pre-existing `agent_dispatch_*` failures; the same representative failure reproduces from the untouched `7164b33` baseline worktree, and this branch has no `agent_runtime` diff.

Every test added in these slices must include the repository's full scenario/data/execution/expected-result comment and explicit assertion messages.
