# Canvas Session Persistence — Phase 1 Design

**Date**: 2026-06-27  
**Status**: Approved  
**Scope**: Phase 1 — Terminal cwd recovery + Claude Agent session resume

---

## Context

Multisoul Desktop currently loses all runtime state on app close. Canvas layout (node positions, viewport) is already persisted via `canvas_snapshots`. Terminal sessions are re-spawned from `session_checkpoints` (shell path + grid size), but each restart opens a fresh shell with no memory of previous work. Agent (Claude/Codex) nodes also start fresh, losing conversation context.

The goal of Phase 1 is **view-restoration mode** (inspired by tmux): on restart, terminals return to the last working directory and Claude agent nodes resume their previous conversation session. No background process keep-alive is required.

---

## Phase 1 Scope

**Included:**
1. Terminal cwd recovery — capture via macOS `proc_pidinfo`, restore via `cd <cwd>` in new shell
2. Claude Agent session resume — read `~/.claude/projects/<encoded>/` for latest session ID, restore via `--resume <id>`

**Deferred:**
- Terminal scrollback text display (user does not need this)
- Codex session resume (file location not project-scoped; TODO Phase 1b)
- Periodic checkpoints (save only on exit)

---

## Architecture

### Mode: View Restoration

```
App exit  → capture cwd + session_id → persist to node_sessions
App open  → load node_sessions → new shell with cwd / claude --resume <id>
```

Processes are killed on close. No background daemon required.

### Storage

New `node_sessions` table in the project SQLite (`<workspace>/.multisoul/project.sqlite`). Kept separate from `session_checkpoints` because they serve different purposes: `session_checkpoints` holds spawn params (shell, grid size); `node_sessions` holds ephemeral runtime state (cwd, session token) captured at exit.

---

## Data Model

### Table: `node_sessions`

```sql
CREATE TABLE IF NOT EXISTS node_sessions (
    node_id         INTEGER PRIMARY KEY NOT NULL,
    kind            TEXT    NOT NULL,    -- "terminal" | "agent"
    cwd             TEXT,                -- NULL if capture failed
    session_id      TEXT,                -- NULL for terminals / if discovery failed
    updated_at_unix INTEGER NOT NULL
);
```

Full `INSERT OR REPLACE` on every exit — no row accumulation.

### Rust Types (`crates/storage/src/node_session_store.rs`)

```rust
pub struct NodeSession {
    pub node_id: NodeId,
    pub kind: NodeSessionKind,
    pub cwd: Option<String>,
    pub session_id: Option<String>,
}

pub enum NodeSessionKind { Terminal, Agent }

pub struct NodeSessionStore { path: PathBuf }
// Methods: save_session(&NodeSession), load_sessions() -> Vec<NodeSession>
```

---

## Save Flow (on app exit)

`lifecycle.rs` already calls `flush_now()` before `kill_all()`. Phase 1 inserts one step:

```
flush_now(runtime)
├── save_canvas_snapshot()              ← existing
├── save_session_checkpoints()          ← existing
└── save_node_sessions()                ← NEW
    ├── For each live Term node:
    │   child_pid → proc_pidinfo(pid) → cwd
    │   → NodeSession { kind: Terminal, cwd, session_id: None }
    └── For each live Agent(Claude) node:
        workspace_root → encode path → scan ~/.claude/projects/<encoded>/
        → most-recently-modified .jsonl stem = session_id
        → NodeSession { kind: Agent, cwd: None, session_id }
kill_all()                              ← existing, after flush
```

### cwd capture: `proc_pidinfo`

`PtyHandle` gains `child_pid: Option<u32>` populated at spawn time via `portable_pty::Child::process_id()`. The cwd is read at exit time via macOS kernel API:

```rust
// crates/runtime/src/macos_proc.rs
#[cfg(target_os = "macos")]
pub(crate) fn process_cwd(pid: u32) -> Option<String> { /* proc_pidinfo PROC_PIDVNODEPATHINFO */ }

#[cfg(not(target_os = "macos"))]
pub(crate) fn process_cwd(_pid: u32) -> Option<String> { None }
```

Requires `libc` as a `[target.'cfg(target_os = "macos")'.dependencies]` in `crates/runtime/Cargo.toml`.

### Claude session ID discovery

Path encoding confirmed from `~/.claude/projects/` on development machine:
```
/Users/alan/Documents/mybook/multisoul-desktop
  → -Users-alan-Documents-mybook-multisoul-desktop
```
Rule: strip leading `/`, replace all `/` with `-`.

Session files are UUIDs: `29822ea0-e447-4c6b-aa4f-2dc827bf0221.jsonl`. The file stem IS the session_id. Pick the file with the latest modification time.

```rust
fn find_agent_session_id(profile: AgentProfile, workspace_root: Option<&Path>) -> Option<String>
```

Returns `None` for `AgentProfile::Codex` with a `// TODO(Phase 1b): implement Codex session discovery` comment.

---

## Restore Flow (on app open)

```
1. load canvas_snapshots      → node layout + viewport       ← existing
2. load session_checkpoints   → shell / cols / rows          ← existing
3. load node_sessions         → cwd + session_id             ← NEW
4. restore_live_sessions_with_node_sessions(checkpoints, node_sessions):
   ├── Term node:
   │   PtyCommand.cwd = NodeSession.cwd  (None → workspace root)
   └── Agent(Claude) node:
       ├── session_id present  → "zsh -ic exec claude --dangerously-skip-permissions --resume <id>"
       └── session_id absent   → original command (fresh session)
```

### Failure handling — all cases degrade silently

| Failure | Behavior |
|---|---|
| `proc_pidinfo` returns `None` | `cwd = NULL` → terminal opens at workspace root |
| Stored cwd no longer exists | Shell startup error visible in terminal; not a crash |
| No `.jsonl` files found | `session_id = NULL` → fresh Claude session |
| `--resume <stale_id>` | Claude TUI shows "session not found"; user handles it |
| No `node_sessions` rows at all | Identical to today's behavior |

---

## File Changes

| File | Change |
|---|---|
| `storage/src/migrations.rs` | Add `node_sessions` table to `migrate_project_db` |
| `storage/src/node_session_store.rs` | **New** — `NodeSession`, `NodeSessionKind`, `NodeSessionStore` |
| `storage/src/lib.rs` | Export new types |
| `storage/src/project_store.rs` | Add `node_session_store()` factory method |
| `terminal/src/pty.rs` | Add `child_pid: Option<u32>` to `PtyHandle` (471 lines → safe) |
| `terminal/src/session.rs` | Add `child_pid()` accessor |
| `runtime/src/live.rs` | Add `fn child_pid() -> Option<u32> { None }` default to `LiveNode` trait |
| `runtime/src/nodes/term.rs` | Override `child_pid()` via `self.session.child_pid()` |
| `runtime/src/macos_proc.rs` | **New** — `proc_pidinfo` binding (~70 lines) |
| `runtime/src/session_snapshot.rs` | **Implement existing stub** — `collect_node_sessions`, `find_agent_session_id`, `build_resume_command`, `build_terminal_restore_command` (~180 lines) |
| `runtime/src/session_restore.rs` | **New** — extract existing save/restore methods from `runtime.rs` + add new methods (~200 lines) |
| `runtime/src/runtime.rs` | Delegate to `session_restore.rs`; keep ≤ 500 lines |
| `app/src/workspace/persistence.rs` | Add `save_node_sessions` call in `flush_now` |
| `app/src/workspace/launchpad.rs` | Load `node_sessions` and pass to restore method |

**Note**: `runtime.rs` is currently 491 lines. The existing `save_session_checkpoints` / `restore_live_sessions` methods (~95 lines) must be extracted to `session_restore.rs` as part of this work to remain under the 500-line constraint.

---

## Implementation Order

Dependencies flow in this sequence:

1. `storage` migration + `NodeSessionStore` — fully isolated, no other crate changes
2. `terminal/src/pty.rs` + `session.rs` — add `child_pid` field and accessor
3. `runtime/src/live.rs` + `nodes/term.rs` — thread `child_pid` up through trait
4. `runtime/src/macos_proc.rs` — kernel binding
5. `runtime/src/session_snapshot.rs` — pure logic; depends on steps 3–4
6. `runtime/src/session_restore.rs` — extract + extend; depends on step 5
7. `app` wiring — `persistence.rs` + `launchpad.rs`

---

## Test Strategy

All tests follow `docs/quality/test-requirements.md`: doc comment with scenario/data/execution/expected, explicit positive and negative assertions with messages.

### Storage (`tests/unit/node_session_store_tests.rs`)
- NSS-1: upsert — two saves of same `node_id` produce one row
- NSS-2: `cwd=None` round-trips as NULL
- NSS-3: `session_id=Some("abc")` round-trips
- NSS-4: empty DB returns empty Vec (backward compat)

### Migration (`tests/unit/migrations_tests.rs`)
- MIG-N: fresh DB has `node_sessions` with expected columns
- MIG-N+1: idempotent — calling migrate twice does not error

### macOS proc (`tests/unit/macos_proc_tests.rs`)
- MPC-1: `process_cwd(std::process::id())` equals `std::env::current_dir()`
- MPC-2: `process_cwd(u32::MAX)` returns `None` without panic

### Session snapshot (`tests/unit/session_snapshot_tests.rs`)
- SS-1: `find_agent_session_id(Claude, workspace)` with temp dir + one `.jsonl` returns its stem
- SS-2: multiple `.jsonl` files → returns stem of most recently modified
- SS-3: workspace root `None` → `None`
- SS-4: projects directory absent → `None`
- SS-5: `build_resume_command(Claude, "abc123", None)` args contain `--resume abc123`
- SS-6: `build_terminal_restore_command("/bin/zsh", "/tmp/x")` has `cwd = /tmp/x`

### Runtime integration (`tests/unit/runtime_storage_tests.rs`)
- RS-7: restore with `cwd` present → terminal opens in that cwd (PTY integration test)
- RS-8: restore with `session_id=None` → agent spawns without `--resume`
- RS-9: restore with `cwd=None` → no error, terminal opens at default

---

## Risks

| Risk | Mitigation |
|---|---|
| Claude path encoding changes between versions | SS-1 test verifies against real encoding; `find_agent_session_id` can try multiple candidate encodings if needed |
| `portable_pty::Child::process_id()` not exposed via `ChildKiller` | Capture PID before calling `clone_killer()` in `spawn_command` — order is safe |
| `proc_pidinfo` struct layout mismatch | MPC-1 test catches this immediately; layout from `/usr/include/sys/proc_info.h` |
| `proc_pidinfo` under App Sandbox (Phase 6) | Add `// TODO(Phase 6): verify proc_pidinfo works under App Sandbox` in `macos_proc.rs` |
| Codex session location not project-scoped | Return `None` for Phase 1; TODO comment for Phase 1b |

---

## Verification

**Manual smoke test (end-to-end)**:
1. `cargo run -p app` — open a project
2. Create a terminal node, `cd /tmp`, run a command
3. Create a Claude agent node, send one message (starts a session)
4. Close the app window
5. `cargo run -p app` — reopen the same project
6. **Terminal node**: verify shell opens in `/tmp`
7. **Claude agent node**: verify `claude --resume <id>` is passed (visible in `RUST_LOG=trace` output)

**Automated**:
```bash
cargo test -p storage -p terminal -p runtime
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
cargo check -p app
```
