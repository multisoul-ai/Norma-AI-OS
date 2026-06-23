# Runtime Card Event Pattern Implementation Plan

## Goal

Add a lightweight runtime event queue and node run-state registry so the app can query live card status and react to lifecycle changes after each `Runtime::drain_all()` pass.

## Scope

- Cover Terminal and Agent live nodes only.
- Keep PTY output chunks on the existing terminal grid path.
- Keep storage writes out of runtime hot paths.
- Preserve `drain_all() -> bool` for existing app repaint logic.

## Steps

1. Add runtime event/state types and bounded queue helpers.
2. Add focused runtime tests for registration, exit transitions, repeated drain idempotence, remove events, operation failures, and queue overflow.
3. Wire spawn, restore, drain, remove, input, resize, and scroll paths to update node states and emit events.
4. Export the new API through the `runtime` crate.
5. Consume events in the app drain loop for tracing/debug and repaint notification.
6. Run `cargo test -p runtime`, `cargo check -p app`, and repository boundary/lint scripts.

## Acceptance Criteria

- `node_run_state(node_id)` returns `Running` after successful Terminal/Agent registration.
- Zero exit transitions to `Finished { code: Some(0) }`.
- Non-zero exit transitions to `Error { message }`.
- Repeated drain calls do not emit duplicate state-change events.
- Removing a live node emits `NodeRemoved` and clears its run state.
- Failed input/resize/scroll operations emit `NodeOperationFailed` without changing run state.
- Event queue remains bounded and emits `EventsDropped` once it exceeds the cap.
