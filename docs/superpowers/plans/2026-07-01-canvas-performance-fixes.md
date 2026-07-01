# Canvas Performance Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove UI-thread stalls from card dragging and canvas-window shutdown.

**Architecture:** Keep persistence and session discovery off hot UI paths. Make session persistence batch and prune stale rows. Reduce drag repaint cost by preventing avoidable terminal body work and culling earlier.

**Tech Stack:** Rust 2021, GPUI 0.2.2, rusqlite, portable-pty.

## Global Constraints

- Write failing tests before production changes.
- Do not revert unrelated dirty worktree changes.
- Do not add `#[allow(...)]`.
- Keep app/storage/runtime dependency direction intact.

---

### Task 1: Stop Close-Time Agent Session Scans

**Files:**
- Modify: `crates/runtime/src/session_snapshot.rs`
- Modify: `crates/runtime/src/session_restore.rs`
- Modify: `crates/runtime/src/runtime.rs`
- Modify: `crates/app/src/settings/agent_hooks.rs`
- Test: `crates/runtime/tests/unit/session_snapshot_tests.rs`
- Test: `crates/runtime/tests/unit/agent_task_event_tests.rs`
- Test: `crates/app/tests/unit/agent_hooks.rs`

**Interfaces:**
- Produce: `collect_node_sessions_with_discovery(..., discover_agent_sessions: bool) -> Vec<NodeSession>`
- Produce: runtime stores hook-provided agent `session_id` by node id.

- [ ] Write failing tests proving snapshot collection can skip filesystem discovery and still use hook-cached session ids.
- [ ] Run focused runtime/app tests and confirm RED.
- [ ] Add runtime session-id cache updated from `AgentTaskUpdate`.
- [ ] Add no-discovery node-session collection path for UI persistence.
- [ ] Run focused tests and confirm GREEN.

### Task 2: Batch and Prune Session Persistence

**Files:**
- Modify: `crates/storage/src/node_session_store.rs`
- Modify: `crates/storage/src/session_store.rs`
- Modify: `crates/runtime/src/session_restore.rs`
- Test: `crates/storage/tests/unit/node_session_store_tests.rs`
- Test: `crates/storage/tests/unit/session_store_tests.rs`

**Interfaces:**
- Produce: `replace_all_sessions(&[NodeSession])`
- Produce: `replace_all_checkpoints(&[SessionCheckpoint])`

- [ ] Write failing storage tests showing stale rows are removed.
- [ ] Run focused storage tests and confirm RED.
- [ ] Implement delete-and-insert transaction replacement APIs.
- [ ] Switch runtime save helpers to replacement APIs.
- [ ] Run focused storage/runtime tests and confirm GREEN.

### Task 3: Reduce Drag Render Cost

**Files:**
- Modify: `crates/app/src/canvas/view.rs`
- Test: `crates/app/tests/unit/canvas_view_tests.rs`

**Interfaces:**
- Produce: early culling before node clone/chrome/body construction.
- Produce: dragged-card source marker that keeps terminal body work gated and testable.

- [ ] Write failing tests proving culling happens before full node clone/chrome work and drag render path has an explicit performance guard.
- [ ] Run focused app tests and confirm RED.
- [ ] Reorder `render_canvas` to cull using borrowed node data before clone/chrome/body.
- [ ] Keep dragged-card motion sync but avoid unrelated body work where possible in this pass.
- [ ] Run focused app tests and confirm GREEN.

### Task 4: Lint and Documentation Guardrails

**Files:**
- Modify: `AGENTS.md`
- Modify: `CLAUDE.md`
- Create: `scripts/check-performance-guardrails.sh`
- Modify: existing hook/test script references if needed.

**Interfaces:**
- Produce: a shell lint that fails on known dangerous patterns in UI hot paths.
- Produce: docs with bad/good examples for teardown and drag rendering.

- [ ] Write shell lint checks for no-op persistence fast return and no session discovery in save/close paths.
- [ ] Add checks that `canvas/events.rs` hot path does not call storage/session discovery APIs.
- [ ] Document performance rules in `AGENTS.md` as short pointers and `CLAUDE.md` as expanded examples.
- [ ] Run the lint and confirm GREEN.

### Task 5: Verification

**Files:**
- No new production files.

- [ ] Run `cargo test -p storage`.
- [ ] Run `cargo test -p runtime`.
- [ ] Run `cargo test -p app`.
- [ ] Run `cargo fmt --check`.
- [ ] Run `bash scripts/check-no-allow.sh`.
- [ ] Run `bash scripts/check-storage-boundaries.sh`.
- [ ] Run `bash scripts/check-performance-guardrails.sh`.
