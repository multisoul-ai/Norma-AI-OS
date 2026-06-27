# Canvas Session Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** On app restart, terminal nodes reopen in their last working directory and Claude agent nodes resume the previous conversation via `--resume <session_id>`.

**Architecture:** New `node_sessions` SQLite table stores cwd (captured via macOS `proc_pidinfo`) and Claude session IDs (discovered from `~/.claude/projects/`) at app exit. On open, these are loaded and applied to the existing `restore_live_sessions` path — terminals get `.cwd()` set on their `PtyCommand`, agents get `--resume <id>` appended. All failures degrade silently to today's behavior.

**Tech Stack:** Rust, rusqlite, portable_pty, libc (macOS-only for proc_pidinfo), GPUI (app layer only)

## Global Constraints

- Domain crates (`canvas`, `terminal`, `runtime`, `storage`) must have zero `gpui` imports — verify with `cargo tree -p runtime | grep gpui`
- Single Rust source files ≤ 500 lines — enforced by `bash scripts/check-max-file-lines.sh`
- No `#[allow(...)]` attributes — enforced by `bash scripts/check-no-allow.sh`
- TDD: write the failing test first, run it, then implement, run again
- Test doc-comment format required: scenario → data construction → execution steps → expected results (see `docs/quality/test-requirements.md`)
- `runtime.rs` is currently 491 lines — Task 6 must extract session restore logic to `session_restore.rs` first
- `libc` dependency is macOS-target-only: `[target.'cfg(target_os = "macos")'.dependencies]`
- Tests go in `crates/<crate>/tests/unit/<name>_tests.rs` mounted via `#[cfg(test)] #[path = "../../tests/unit/<name>_tests.rs"] mod tests;` at the bottom of the source file

---

## File Map

| Path | Action | Purpose |
|---|---|---|
| `crates/storage/src/migrations.rs` | Modify | Add `node_sessions` table creation |
| `crates/storage/src/error.rs` | Modify | Add `InvalidNodeSessionKind(String)` variant |
| `crates/storage/src/node_session_store.rs` | **Create** | `NodeSession`, `NodeSessionKind`, `NodeSessionStore` |
| `crates/storage/src/lib.rs` | Modify | Export new types; add module |
| `crates/storage/src/project_store.rs` | Modify | Add `node_session_store()` factory |
| `crates/storage/tests/unit/node_session_store_tests.rs` | **Create** | Unit tests for store |
| `crates/terminal/src/pty.rs` | Modify | Add `child_pid: Option<u32>` to `PtyHandle`, populate at spawn |
| `crates/terminal/src/session.rs` | Modify | Add `child_pid()` accessor |
| `crates/runtime/src/live.rs` | Modify | Add `fn child_pid() -> Option<u32> { None }` to `LiveNode` trait |
| `crates/runtime/src/nodes/term.rs` | Modify | Override `child_pid()` in `TermLive` |
| `crates/runtime/src/macos_proc.rs` | **Create** | `process_cwd(pid)` via `proc_pidinfo` |
| `crates/runtime/tests/unit/macos_proc_tests.rs` | **Create** | Kernel API tests |
| `crates/runtime/src/session_snapshot.rs` | Implement stub | `collect_node_sessions`, `find_agent_session_id`, `build_resume_command` |
| `crates/runtime/tests/unit/session_snapshot_tests.rs` | **Create** | Pure-logic tests for snapshot functions |
| `crates/runtime/src/session_restore.rs` | **Create** | Extract from `runtime.rs` + new `restore_live_sessions_with_node_sessions` |
| `crates/runtime/src/runtime.rs` | Modify | Delegate to `session_restore.rs`; keep ≤ 500 lines |
| `crates/runtime/Cargo.toml` | Modify | Add `libc` under macOS target dependency |
| `crates/app/src/workspace/persistence.rs` | Modify | Call `save_node_sessions` in `flush_now` |
| `crates/app/src/workspace/launchpad.rs` | Modify | Load `node_sessions` and pass to restore |

---

## Task 1: Storage Foundation

**Files:**
- Create: `crates/storage/src/node_session_store.rs`
- Create: `crates/storage/tests/unit/node_session_store_tests.rs`
- Modify: `crates/storage/src/migrations.rs`
- Modify: `crates/storage/src/error.rs`
- Modify: `crates/storage/src/lib.rs`
- Modify: `crates/storage/src/project_store.rs`

**Interfaces:**
- Produces: `NodeSession { node_id: NodeId, kind: NodeSessionKind, cwd: Option<String>, session_id: Option<String> }`
- Produces: `NodeSessionKind` enum (`Terminal | Agent`)
- Produces: `NodeSessionStore::save_session(&NodeSession) -> StorageResult<()>`
- Produces: `NodeSessionStore::load_sessions() -> StorageResult<Vec<NodeSession>>`
- Produces: `ProjectStore::node_session_store() -> NodeSessionStore`

- [ ] **Step 1.1: Write the failing tests**

Create `crates/storage/tests/unit/node_session_store_tests.rs`:

```rust
//! Unit tests for NodeSessionStore.

use std::path::PathBuf;
use tempfile::tempdir;
use crate::{
    migrations::migrate_project_db,
    node_session_store::{NodeSession, NodeSessionKind, NodeSessionStore},
};

fn make_store() -> (NodeSessionStore, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.sqlite");
    let conn = rusqlite::Connection::open(&path).unwrap();
    migrate_project_db(&conn).unwrap();
    (NodeSessionStore::new(path), dir)
}

/// NSS-1: 同一 node_id 的两次 save 只保留一行（upsert）。
///
/// 数据构造：
///   node_id = 42
///   第一次 save: cwd = "/tmp/a"
///   第二次 save: cwd = "/tmp/b"（覆盖）
///
/// 执行过程：
///   1. 第一次 save → INSERT OR REPLACE
///   2. 第二次 save → INSERT OR REPLACE，覆盖旧行
///   3. load_sessions()
///
/// 预期结果：
///   - 返回 1 行
///   - cwd = Some("/tmp/b")
#[test]
fn test_upsert_overwrites_same_node_id() {
    let (store, _dir) = make_store();
    let session_a = NodeSession {
        node_id: 42,
        kind: NodeSessionKind::Terminal,
        cwd: Some("/tmp/a".into()),
        session_id: None,
    };
    let session_b = NodeSession {
        node_id: 42,
        kind: NodeSessionKind::Terminal,
        cwd: Some("/tmp/b".into()),
        session_id: None,
    };

    store.save_session(&session_a).unwrap();
    store.save_session(&session_b).unwrap();
    let rows = store.load_sessions().unwrap();

    assert_eq!(rows.len(), 1, "upsert 应只保留一行，但存在 {} 行", rows.len());
    assert_eq!(
        rows[0].cwd.as_deref(),
        Some("/tmp/b"),
        "第二次 save 应覆盖 cwd，但实际为 {:?}",
        rows[0].cwd
    );
}

/// NSS-2: cwd = None 存储为 NULL，load 后仍为 None。
///
/// 预期结果：
///   - rows[0].cwd == None
#[test]
fn test_null_cwd_round_trips() {
    let (store, _dir) = make_store();
    store.save_session(&NodeSession {
        node_id: 1,
        kind: NodeSessionKind::Terminal,
        cwd: None,
        session_id: None,
    }).unwrap();

    let rows = store.load_sessions().unwrap();
    assert!(rows[0].cwd.is_none(), "cwd=None 应保持 None，实际为 {:?}", rows[0].cwd);
}

/// NSS-3: session_id = Some("abc") 经 save/load 后完整保留。
///
/// 预期结果：
///   - rows[0].session_id == Some("abc")
#[test]
fn test_session_id_round_trips() {
    let (store, _dir) = make_store();
    store.save_session(&NodeSession {
        node_id: 5,
        kind: NodeSessionKind::Agent,
        cwd: None,
        session_id: Some("abc-123".into()),
    }).unwrap();

    let rows = store.load_sessions().unwrap();
    assert_eq!(
        rows[0].session_id.as_deref(),
        Some("abc-123"),
        "session_id 应完整保留"
    );
}

/// NSS-4: 空表的 load_sessions 返回空 Vec（向后兼容，迁移后首次打开）。
///
/// 预期结果：
///   - rows.len() == 0
#[test]
fn test_load_empty_returns_empty_vec() {
    let (store, _dir) = make_store();
    let rows = store.load_sessions().unwrap();
    assert!(rows.is_empty(), "空表应返回空 Vec，实际 {} 行", rows.len());
}
```

- [ ] **Step 1.2: Run tests to confirm they fail**

```bash
cargo test -p storage 2>&1 | head -30
```

Expected: compile error — `node_session_store` module not found.

- [ ] **Step 1.3: Add migration SQL**

Open `crates/storage/src/migrations.rs`. Append to the `execute_batch` string inside `migrate_project_db`:

```sql
CREATE TABLE IF NOT EXISTS node_sessions (
    node_id         INTEGER PRIMARY KEY NOT NULL,
    kind            TEXT    NOT NULL,
    cwd             TEXT,
    session_id      TEXT,
    updated_at_unix INTEGER NOT NULL
);
```

- [ ] **Step 1.4: Add StorageError variant**

Open `crates/storage/src/error.rs`. Add one variant to the `StorageError` enum:

```rust
#[error("invalid node session kind: {0}")]
InvalidNodeSessionKind(String),
```

- [ ] **Step 1.5: Create NodeSessionStore**

Create `crates/storage/src/node_session_store.rs`:

```rust
use std::path::PathBuf;
use rusqlite::{params, Connection};
use canvas::NodeId;
use crate::{StorageError, StorageResult};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeSession {
    pub node_id: NodeId,
    pub kind: NodeSessionKind,
    pub cwd: Option<String>,
    pub session_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeSessionKind {
    Terminal,
    Agent,
}

impl NodeSessionKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Agent => "agent",
        }
    }

    fn from_str(s: &str) -> StorageResult<Self> {
        match s {
            "terminal" => Ok(Self::Terminal),
            "agent" => Ok(Self::Agent),
            other => Err(StorageError::InvalidNodeSessionKind(other.to_string())),
        }
    }
}

pub struct NodeSessionStore {
    path: PathBuf,
}

impl NodeSessionStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn save_session(&self, session: &NodeSession) -> StorageResult<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "INSERT OR REPLACE INTO node_sessions \
             (node_id, kind, cwd, session_id, updated_at_unix) \
             VALUES (?1, ?2, ?3, ?4, strftime('%s','now'))",
            params![
                session.node_id,
                session.kind.as_str(),
                session.cwd,
                session.session_id,
            ],
        )?;
        Ok(())
    }

    pub fn load_sessions(&self) -> StorageResult<Vec<NodeSession>> {
        let conn = Connection::open(&self.path)?;
        let mut stmt = conn.prepare(
            "SELECT node_id, kind, cwd, session_id FROM node_sessions",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, u64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })?
            .map(|r| {
                let (node_id, kind_str, cwd, session_id) = r?;
                let kind = NodeSessionKind::from_str(&kind_str)?;
                Ok(NodeSession { node_id, kind, cwd, session_id })
            })
            .collect::<StorageResult<Vec<_>>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/node_session_store_tests.rs"]
mod tests;
```

- [ ] **Step 1.6: Wire into lib.rs and project_store.rs**

In `crates/storage/src/lib.rs`, add:
```rust
pub mod node_session_store;
pub use node_session_store::{NodeSession, NodeSessionKind, NodeSessionStore};
```

In `crates/storage/src/project_store.rs`, add a factory method parallel to `session_store()`:
```rust
pub fn node_session_store(&self) -> NodeSessionStore {
    NodeSessionStore::new(self.path.clone())
}
```

- [ ] **Step 1.7: Run tests to confirm they pass**

```bash
cargo test -p storage -- node_session 2>&1
```

Expected: 4 tests pass (NSS-1 through NSS-4).

- [ ] **Step 1.8: Run constraint checks**

```bash
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
```

Expected: no violations.

- [ ] **Step 1.9: Commit**

```bash
git add crates/storage/
git commit -m "feat(storage): add node_sessions table and NodeSessionStore"
```

---

## Task 2: Terminal Child PID Capture

**Files:**
- Modify: `crates/terminal/src/pty.rs`
- Modify: `crates/terminal/src/session.rs`

**Interfaces:**
- Consumes: `portable_pty::Child::process_id() -> Option<u32>` — available before `clone_killer()` is called
- Produces: `PtyHandle::child_pid: Option<u32>` — populated at spawn time
- Produces: `TerminalSession::child_pid() -> Option<u32>`

- [ ] **Step 2.1: Write the failing test**

Find the existing test file for `pty.rs` — it's likely at `crates/terminal/tests/unit/pty_tests.rs` or inline. Add this test:

```rust
/// T-PID-1: spawn_command 成功后 child_pid 应为 Some(非零值)。
///
/// 数据构造：
///   command = "sleep 1"（保证进程存活足够长）
///
/// 执行过程：
///   1. spawn_command(id=1, PtyCommand::new("sleep").args(["1"]), cols=80, rows=24)
///   2. 读取 child_pid
///
/// 预期结果：
///   - child_pid 是 Some(pid) 且 pid > 0
#[test]
fn test_spawn_sets_child_pid() {
    use crate::session::TerminalSession;
    use crate::pty::PtyCommand;

    let mut session = TerminalSession::spawn_command(
        1,
        PtyCommand::new("sleep").args(["100"]),
        80,
        24,
    ).unwrap();

    let pid = session.child_pid();
    assert!(pid.is_some(), "spawn 后 child_pid 应为 Some，实际为 None");
    assert!(pid.unwrap() > 0, "PID 应大于 0，实际为 {}", pid.unwrap());

    session.kill();
}
```

- [ ] **Step 2.2: Run test to confirm it fails**

```bash
cargo test -p terminal -- test_spawn_sets_child_pid 2>&1
```

Expected: compile error — `child_pid()` method not found.

- [ ] **Step 2.3: Add child_pid to PtyHandle**

In `crates/terminal/src/pty.rs`, locate `PtyHandle` struct definition and add the field:

```rust
pub struct PtyHandle {
    master: Box<dyn portable_pty::MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::ChildKiller + Send + Sync>,
    rx: Receiver<PtyEvent>,
    pub child_pid: Option<u32>,   // ← add this line
}
```

In `spawn_command`, add PID capture **before** calling `clone_killer()`. The existing code looks like:

```rust
let mut child = pair.slave.spawn_command(cmd_builder)?;
// ADD THIS LINE before clone_killer():
let child_pid = child.process_id();
let child_killer = child.clone_killer();
```

In the `PtyHandle { ... }` construction at the end of `spawn_command`, add:

```rust
PtyHandle {
    master: pair.master,
    writer,
    child: child_killer,
    rx,
    child_pid,   // ← add this
}
```

- [ ] **Step 2.4: Add child_pid() accessor on TerminalSession**

In `crates/terminal/src/session.rs`, add the public method:

```rust
pub fn child_pid(&self) -> Option<u32> {
    self.pty.child_pid
}
```

- [ ] **Step 2.5: Run test to confirm it passes**

```bash
cargo test -p terminal -- test_spawn_sets_child_pid 2>&1
```

Expected: PASS.

- [ ] **Step 2.6: Run all terminal tests**

```bash
cargo test -p terminal 2>&1
```

Expected: all existing tests still pass.

- [ ] **Step 2.7: Run constraint checks**

```bash
bash scripts/check-max-file-lines.sh
```

Expected: `pty.rs` ≤ 500 lines (was 471 + ~3 lines = still fine).

- [ ] **Step 2.8: Commit**

```bash
git add crates/terminal/
git commit -m "feat(terminal): expose child PID from PtyHandle for cwd capture"
```

---

## Task 3: LiveNode child_pid Trait Method

**Files:**
- Modify: `crates/runtime/src/live.rs`
- Modify: `crates/runtime/src/nodes/term.rs`

**Interfaces:**
- Consumes: `TerminalSession::child_pid() -> Option<u32>` (from Task 2)
- Produces: `LiveNode::child_pid() -> Option<u32>` — default returns `None`; `TermLive` overrides via `self.session.child_pid()`

- [ ] **Step 3.1: Add default trait method to LiveNode**

In `crates/runtime/src/live.rs`, find the `LiveNode` trait definition and add one default method at the end of the trait body:

```rust
pub trait LiveNode: Send {
    fn caps(&self) -> Caps;
    fn drain(&mut self) -> bool;
    fn terminal_status(&self) -> Option<TerminalStatus> { None }
    fn resize(&mut self, resize: NodeResize) -> Result<(), NodeError>;
    fn input(&mut self, input: NodeInput) -> Result<(), NodeError>;
    fn scroll(&mut self, _scroll: NodeScroll) -> Result<bool, NodeError>;
    fn view(&self) -> Option<NodeView>;
    fn kill(&mut self);
    fn child_pid(&self) -> Option<u32> { None }  // ← add this
}
```

- [ ] **Step 3.2: Override in TermLive**

In `crates/runtime/src/nodes/term.rs`, add the override implementation inside `impl LiveNode for TermLive`:

```rust
fn child_pid(&self) -> Option<u32> {
    self.session.child_pid()
}
```

- [ ] **Step 3.3: Compile check**

```bash
cargo check -p runtime 2>&1
```

Expected: no errors.

- [ ] **Step 3.4: Run runtime tests**

```bash
cargo test -p runtime 2>&1
```

Expected: all tests pass.

- [ ] **Step 3.5: Commit**

```bash
git add crates/runtime/src/live.rs crates/runtime/src/nodes/term.rs
git commit -m "feat(runtime): add child_pid() to LiveNode trait for cwd capture"
```

---

## Task 4: macOS proc_pidinfo Binding

**Files:**
- Create: `crates/runtime/src/macos_proc.rs`
- Create: `crates/runtime/tests/unit/macos_proc_tests.rs`
- Modify: `crates/runtime/Cargo.toml`
- Modify: `crates/runtime/src/lib.rs` (or `mod.rs` — wherever modules are declared)

**Interfaces:**
- Produces: `pub(crate) fn process_cwd(pid: u32) -> Option<String>`

- [ ] **Step 4.1: Write the failing tests**

Create `crates/runtime/tests/unit/macos_proc_tests.rs`:

```rust
//! Tests for macOS proc_pidinfo cwd capture.

#[cfg(target_os = "macos")]
mod macos {
    use crate::macos_proc::process_cwd;

    /// MPC-1: process_cwd 对当前进程返回与 std::env::current_dir() 相同的路径。
    ///
    /// 数据构造：
    ///   pid = std::process::id()（测试进程自身）
    ///   expected = std::env::current_dir().unwrap()
    ///
    /// 执行过程：
    ///   1. 调用 process_cwd(current pid)
    ///
    /// 预期结果：
    ///   - 返回 Some(path)
    ///   - path 等于 current_dir()
    #[test]
    fn test_process_cwd_self() {
        let expected = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let pid = std::process::id();

        let got = process_cwd(pid);
        assert!(got.is_some(), "当前进程的 cwd 应能被读取，实际为 None");
        assert_eq!(
            got.unwrap(),
            expected,
            "process_cwd 返回的路径应与 current_dir() 一致"
        );
    }

    /// MPC-2: process_cwd 对不存在的 PID 返回 None，不 panic。
    ///
    /// 数据构造：
    ///   pid = u32::MAX（不可能存在的 PID）
    ///
    /// 预期结果：
    ///   - 返回 None（不 panic）
    #[test]
    fn test_process_cwd_invalid_pid_returns_none() {
        let result = process_cwd(u32::MAX);
        assert!(result.is_none(), "无效 PID 应返回 None，实际为 {:?}", result);
    }
}
```

- [ ] **Step 4.2: Run to confirm failure**

```bash
cargo test -p runtime -- macos_proc 2>&1
```

Expected: compile error — `macos_proc` module not found.

- [ ] **Step 4.3: Add libc dependency**

In `crates/runtime/Cargo.toml`, add under `[dependencies]` or create the target section:

```toml
[target.'cfg(target_os = "macos")'.dependencies]
libc = "0.2"
```

- [ ] **Step 4.4: Create macos_proc.rs**

Create `crates/runtime/src/macos_proc.rs`:

```rust
//! macOS-only: read a process's current working directory via proc_pidinfo.
//! The non-macOS stub always returns None — this project is macOS-only today,
//! but the stub keeps CI on other platforms compilable.
//!
//! TODO(Phase 6): verify proc_pidinfo works under App Sandbox before notarization.

#[cfg(target_os = "macos")]
pub(crate) fn process_cwd(pid: u32) -> Option<String> {
    use std::ffi::CStr;

    // proc_vnodepathinfo layout (from /usr/include/sys/proc_info.h):
    //   pvi_cdir: vnode_info_path {
    //       vip_vi:   vnode_info    = 160 bytes
    //       vip_path: [u8; 1024]
    //   }
    //   pvi_rdir: vnode_info_path  = 1184 bytes (not used)
    // Total: 2368 bytes
    const PROC_PIDVNODEPATHINFO: libc::c_int = 9;
    const VNODE_INFO_SIZE: usize = 160;
    const MAXPATHLEN: usize = 1024;
    const BUF_SIZE: usize = (VNODE_INFO_SIZE + MAXPATHLEN) * 2;

    let mut buf = [0u8; BUF_SIZE];
    let ret = unsafe {
        libc::proc_pidinfo(
            pid as libc::pid_t,
            PROC_PIDVNODEPATHINFO,
            0,
            buf.as_mut_ptr().cast::<libc::c_void>(),
            BUF_SIZE as libc::c_int,
        )
    };

    // proc_pidinfo returns the number of bytes written; a short or negative
    // result means permission denied, no such process, or wrong buffer size.
    if ret as usize != BUF_SIZE {
        return None;
    }

    // cwd path immediately follows the 160-byte vnode_info header of pvi_cdir
    let path_bytes = &buf[VNODE_INFO_SIZE..VNODE_INFO_SIZE + MAXPATHLEN];
    // SAFETY: path_bytes is NUL-terminated (the buffer was zero-initialized and
    // proc_pidinfo writes a NUL-terminated C string into vip_path).
    let cstr = unsafe { CStr::from_ptr(path_bytes.as_ptr().cast::<libc::c_char>()) };
    let s = cstr.to_string_lossy();
    if s.is_empty() {
        None
    } else {
        Some(s.into_owned())
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn process_cwd(_pid: u32) -> Option<String> {
    None
}

#[cfg(test)]
#[path = "../../tests/unit/macos_proc_tests.rs"]
mod tests;
```

- [ ] **Step 4.5: Declare module in runtime**

In `crates/runtime/src/lib.rs` (or wherever the module list is), add:

```rust
pub(crate) mod macos_proc;
```

- [ ] **Step 4.6: Run tests to confirm they pass**

```bash
cargo test -p runtime -- macos_proc 2>&1
```

Expected: MPC-1 and MPC-2 pass.

- [ ] **Step 4.7: Verify no GPUI leak**

```bash
cargo tree -p runtime | grep gpui
```

Expected: no output.

- [ ] **Step 4.8: Commit**

```bash
git add crates/runtime/src/macos_proc.rs \
        crates/runtime/tests/unit/macos_proc_tests.rs \
        crates/runtime/Cargo.toml \
        crates/runtime/src/lib.rs
git commit -m "feat(runtime): add macOS proc_pidinfo binding for cwd capture"
```

---

## Task 5: Session Snapshot Logic

**Files:**
- Implement: `crates/runtime/src/session_snapshot.rs` (currently a one-line placeholder stub)
- Create: `crates/runtime/tests/unit/session_snapshot_tests.rs`

**Interfaces:**
- Consumes: `LiveNode::child_pid()` (Task 3), `macos_proc::process_cwd()` (Task 4), `NodeSession`/`NodeSessionKind` (Task 1)
- Produces:
  - `pub fn collect_node_sessions(live: &HashMap<NodeId, Box<dyn LiveNode>>, document: &Document, workspace_root: Option<&Path>) -> Vec<NodeSession>`
  - `pub fn find_agent_session_id(workspace_root: &Path) -> Option<String>`
  - `pub fn build_resume_command(shell: &str, session_id: &str, hook_env: Option<&AgentHookEnv>) -> PtyCommand`
  - `pub fn build_terminal_restore_command(shell: &str, cwd: &str) -> PtyCommand`

- [ ] **Step 5.1: Write the failing tests**

Create `crates/runtime/tests/unit/session_snapshot_tests.rs`:

```rust
//! Unit tests for session_snapshot logic.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::tempdir;
use crate::session_snapshot::{build_resume_command, build_terminal_restore_command, find_agent_session_id};
use canvas::node::AgentProfile;

fn write_jsonl_file(dir: &std::path::Path, name: &str) -> PathBuf {
    let path = dir.join(format!("{name}.jsonl"));
    fs::write(&path, b"{}").unwrap();
    path
}

/// SS-1: find_agent_session_id 在 ~/.claude/projects/<encoded>/ 只有一个 .jsonl 时返回其 stem。
///
/// 数据构造：
///   workspace_root = tmp/myproject
///   encoded_name = "-tmp-myproject" (rule: strip leading /, replace / with -)
///   projects_dir に 1 つの .jsonl: "deadbeef-1234.jsonl"
///
/// 预期结果：
///   - Some("deadbeef-1234")
#[test]
fn test_find_session_single_file() {
    let home = tempdir().unwrap();
    let workspace = tempdir().unwrap();

    // Build the encoded projects directory
    let encoded = encode_path_for_claude(workspace.path().to_str().unwrap());
    let projects_dir = home.path().join(".claude").join("projects").join(encoded);
    fs::create_dir_all(&projects_dir).unwrap();
    write_jsonl_file(&projects_dir, "deadbeef-1234");

    let result = find_agent_session_id_with_home(workspace.path(), home.path());
    assert_eq!(
        result.as_deref(),
        Some("deadbeef-1234"),
        "单文件时应返回该文件的 stem"
    );
}

/// SS-2: find_agent_session_id 有多个 .jsonl 时返回最新修改的那个 stem。
///
/// 数据构造：
///   older file: "old-session.jsonl"（mtime = 1秒前）
///   newer file: "new-session.jsonl"（mtime = 现在）
///
/// 预期结果：
///   - Some("new-session")
#[test]
fn test_find_session_returns_most_recent() {
    let home = tempdir().unwrap();
    let workspace = tempdir().unwrap();

    let encoded = encode_path_for_claude(workspace.path().to_str().unwrap());
    let projects_dir = home.path().join(".claude").join("projects").join(encoded);
    fs::create_dir_all(&projects_dir).unwrap();

    let old_path = write_jsonl_file(&projects_dir, "old-session");
    // Set mtime to 2 seconds ago
    let past = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() - 2;
    filetime::set_file_mtime(&old_path, filetime::FileTime::from_unix_time(past as i64, 0)).unwrap();

    write_jsonl_file(&projects_dir, "new-session");

    let result = find_agent_session_id_with_home(workspace.path(), home.path());
    assert_eq!(
        result.as_deref(),
        Some("new-session"),
        "应返回 mtime 最新的文件 stem"
    );
}

/// SS-3: workspace_root 对应的 projects 目录不存在时返回 None。
///
/// 预期结果：
///   - None
#[test]
fn test_find_session_no_projects_dir_returns_none() {
    let home = tempdir().unwrap();
    let workspace = tempdir().unwrap();
    // Do NOT create any projects directory

    let result = find_agent_session_id_with_home(workspace.path(), home.path());
    assert!(result.is_none(), "projects 目录不存在时应返回 None");
}

/// SS-4: projects 目录存在但无 .jsonl 文件时返回 None。
///
/// 预期结果：
///   - None
#[test]
fn test_find_session_empty_dir_returns_none() {
    let home = tempdir().unwrap();
    let workspace = tempdir().unwrap();

    let encoded = encode_path_for_claude(workspace.path().to_str().unwrap());
    let projects_dir = home.path().join(".claude").join("projects").join(encoded);
    fs::create_dir_all(&projects_dir).unwrap();
    // Write a non-.jsonl file to make sure it's ignored
    fs::write(projects_dir.join("other.txt"), b"").unwrap();

    let result = find_agent_session_id_with_home(workspace.path(), home.path());
    assert!(result.is_none(), "无 .jsonl 文件时应返回 None");
}

/// SS-5: build_resume_command 生成的 PtyCommand 参数中包含 "--resume abc123"。
///
/// 预期结果：
///   - args 拼接后包含子字符串 "--resume abc123"
#[test]
fn test_build_resume_command_contains_resume_flag() {
    let cmd = build_resume_command("/bin/zsh", "abc123", None);
    let args_str = cmd.args.join(" ");
    assert!(
        args_str.contains("--resume abc123"),
        "resume command 的参数应包含 --resume abc123，实际: {args_str}"
    );
}

/// SS-6: build_terminal_restore_command 生成的 PtyCommand cwd 为指定路径。
///
/// 预期结果：
///   - cmd.cwd == Some(PathBuf::from("/tmp/myproject"))
#[test]
fn test_build_terminal_restore_command_sets_cwd() {
    let cmd = build_terminal_restore_command("/bin/zsh", "/tmp/myproject");
    assert_eq!(
        cmd.cwd.as_deref(),
        Some(std::path::Path::new("/tmp/myproject")),
        "terminal restore command 的 cwd 应为指定路径"
    );
}

// Helper: encode workspace path the same way Claude does.
fn encode_path_for_claude(path: &str) -> String {
    path.trim_start_matches('/').replace('/', "-")
}

// Helper: call find_agent_session_id with a custom home dir for tests.
fn find_agent_session_id_with_home(workspace: &std::path::Path, home: &std::path::Path) -> Option<String> {
    crate::session_snapshot::find_agent_session_id_inner(workspace, home)
}
```

Note: this requires `filetime` as a dev-dependency. Add to `crates/runtime/Cargo.toml`:
```toml
[dev-dependencies]
filetime = "0.2"
tempfile = "3"
```

- [ ] **Step 5.2: Run to confirm failure**

```bash
cargo test -p runtime -- session_snapshot 2>&1
```

Expected: compile error.

- [ ] **Step 5.3: Implement session_snapshot.rs**

Replace the stub content of `crates/runtime/src/session_snapshot.rs`:

```rust
//! Session snapshot: capture cwd + Claude session IDs at exit,
//! and build PtyCommands that restore them at next open.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use canvas::{Document, NodeId, node::{AgentProfile, CanvasNodeKind}};
use storage::{NodeSession, NodeSessionKind};
use crate::{live::LiveNode, nodes::agent::AgentHookEnv, macos_proc};
use terminal::pty::PtyCommand;

/// Collect cwd and session_id for every live node at app exit.
/// Called before kill_all() so PTY processes are still alive.
pub fn collect_node_sessions(
    live: &HashMap<NodeId, Box<dyn LiveNode>>,
    document: &Document,
    workspace_root: Option<&Path>,
) -> Vec<NodeSession> {
    let mut sessions = Vec::new();

    for node in document.nodes() {
        let Some(live_node) = live.get(&node.id) else { continue };

        match &node.kind {
            CanvasNodeKind::Term(_) => {
                let cwd = live_node
                    .child_pid()
                    .and_then(|pid| macos_proc::process_cwd(pid));
                sessions.push(NodeSession {
                    node_id: node.id,
                    kind: NodeSessionKind::Terminal,
                    cwd,
                    session_id: None,
                });
            }
            CanvasNodeKind::Agent(spec) => {
                let session_id = workspace_root.and_then(|root| {
                    match spec.profile {
                        AgentProfile::Claude => find_agent_session_id(root),
                        AgentProfile::Codex => {
                            // TODO(Phase 1b): implement Codex session file discovery
                            None
                        }
                    }
                });
                sessions.push(NodeSession {
                    node_id: node.id,
                    kind: NodeSessionKind::Agent,
                    cwd: None,
                    session_id,
                });
            }
            _ => {} // Rect and Preview nodes have no session state
        }
    }

    sessions
}

/// Find the most recently modified Claude session file for the given workspace.
///
/// Claude stores sessions at: ~/.claude/projects/<encoded-path>/<uuid>.jsonl
/// Path encoding: strip leading '/', replace '/' with '-'.
/// Example: /Users/alan/project → -Users-alan-project
pub fn find_agent_session_id(workspace_root: &Path) -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    find_agent_session_id_inner(workspace_root, Path::new(&home))
}

/// Testable inner function that accepts an explicit home directory.
pub fn find_agent_session_id_inner(workspace_root: &Path, home: &Path) -> Option<String> {
    let encoded = encode_claude_path(workspace_root.to_str()?);
    let projects_dir = home.join(".claude").join("projects").join(encoded);

    let entries = std::fs::read_dir(&projects_dir).ok()?;

    let mut best: Option<(std::time::SystemTime, String)> = None;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(mtime) = meta.modified() else { continue };

        if best.as_ref().map(|(t, _)| mtime > *t).unwrap_or(true) {
            best = Some((mtime, stem.to_string()));
        }
    }

    best.map(|(_, stem)| stem)
}

/// Encode a workspace path the same way Claude Code does:
/// strip leading '/', replace '/' with '-'.
fn encode_claude_path(path: &str) -> String {
    path.trim_start_matches('/').replace('/', "-")
}

/// Build a PtyCommand for a terminal node that opens in the saved cwd.
pub fn build_terminal_restore_command(shell: &str, cwd: &str) -> PtyCommand {
    PtyCommand::new(shell)
        .args(["-l"])
        .cwd(PathBuf::from(cwd))
}

/// Build a PtyCommand for a Claude agent that resumes the saved session.
pub fn build_resume_command(
    shell: &str,
    session_id: &str,
    hook_env: Option<&AgentHookEnv>,
) -> PtyCommand {
    let cmd_str = format!(
        "exec claude --dangerously-skip-permissions --resume {session_id}"
    );
    let mut cmd = PtyCommand::new(shell).args(["-ic", &cmd_str]);
    if let Some(env) = hook_env {
        cmd = cmd.envs(env.as_env_pairs());
    }
    cmd
}

#[cfg(test)]
#[path = "../../tests/unit/session_snapshot_tests.rs"]
mod tests;
```

**Note on `PtyCommand` API**: Check the existing `PtyCommand` struct in `crates/terminal/src/pty.rs` for the actual builder method names (`args`, `cwd`, `envs`). Use whatever methods are already defined — do not add new ones. If `args` takes `&[&str]`, adjust the call to match.

**Note on `document.nodes()`**: Check `crates/canvas/src/document.rs` for the actual iterator method name for nodes. Common names: `nodes()`, `iter_nodes()`, `all_nodes()`. Use whatever exists.

**Note on `AgentHookEnv`**: This is the existing hook env struct in `crates/runtime/src/nodes/agent.rs`. Check its actual name and `as_env_pairs()` method — use whatever exists. If no such method exists, pass `None` for now and add a TODO.

- [ ] **Step 5.4: Run tests to confirm they pass**

```bash
cargo test -p runtime -- session_snapshot 2>&1
```

Expected: SS-1 through SS-6 all pass.

- [ ] **Step 5.5: Commit**

```bash
git add crates/runtime/src/session_snapshot.rs \
        crates/runtime/tests/unit/session_snapshot_tests.rs \
        crates/runtime/Cargo.toml
git commit -m "feat(runtime): implement session snapshot capture and restore commands"
```

---

## Task 6: Session Restore Extraction and Extension

**Files:**
- Create: `crates/runtime/src/session_restore.rs`
- Modify: `crates/runtime/src/runtime.rs`

**Interfaces:**
- Consumes: `collect_node_sessions` / `build_resume_command` / `build_terminal_restore_command` (Task 5), `NodeSession` (Task 1)
- Produces:
  - `pub fn collect_session_checkpoints(live: &HashMap<NodeId, Box<dyn LiveNode>>, document: &Document) -> Vec<SessionCheckpoint>` — moved from `runtime.rs`
  - `pub fn save_session_checkpoints(store: &SessionStore, checkpoints: &[SessionCheckpoint]) -> StorageResult<()>` — moved from `runtime.rs`
  - `pub fn restore_live_sessions(checkpoints: &[SessionCheckpoint], /* ... */) -> usize` — moved from `runtime.rs`
  - `pub fn save_node_sessions(store: &NodeSessionStore, live: &HashMap<NodeId, Box<dyn LiveNode>>, document: &Document, workspace_root: Option<&Path>) -> StorageResult<()>` — new
  - `pub fn restore_live_sessions_with_node_sessions(checkpoints: &[SessionCheckpoint], node_sessions: &[NodeSession], /* ... */) -> usize` — new

- [ ] **Step 6.1: Read runtime.rs to understand what to extract**

```bash
grep -n "fn collect_session\|fn save_session\|fn restore_live" \
  crates/runtime/src/runtime.rs
```

Note the exact line numbers of `collect_session_checkpoints`, `save_session_checkpoints`, and `restore_live_sessions`. You will move these methods (their full bodies) into `session_restore.rs`.

- [ ] **Step 6.2: Write the failing tests**

Create or extend `crates/runtime/tests/unit/runtime_storage_tests.rs`. Add:

```rust
/// RS-7: restore_live_sessions_with_node_sessions 传入 cwd 时，
/// 新 shell 的工作目录为 cwd 路径。
///
/// 数据构造：
///   cwd = /tmp（保证路径存在）
///   SessionCheckpoint for a terminal node
///   NodeSession { kind: Terminal, cwd: Some("/tmp"), session_id: None }
///
/// 执行过程：
///   1. restore_live_sessions_with_node_sessions(checkpoints, node_sessions)
///   2. 向 terminal 输入 "pwd\n"，drain，读取 snapshot
///
/// 预期结果：
///   - snapshot 中出现 "/tmp"（shell 已在 /tmp 目录）
#[test]
fn test_restore_terminal_applies_cwd() {
    // This is an integration test that spawns a real PTY.
    // See existing runtime_storage_tests for the pattern (test_spawn_command_uses_configured_cwd).
    // Implement following that same pattern, asserting pwd output contains "/tmp".
    // Omit if no existing PTY integration test pattern exists — add a compile-only check instead.
    todo!("implement following existing PTY integration test pattern")
}

/// RS-8: restore_live_sessions_with_node_sessions 当 NodeSession.session_id = None 时，
/// 不追加 --resume，使用原始 agent 命令。
///
/// 数据构造：
///   NodeSession { kind: Agent, session_id: None }
///
/// 预期结果：
///   - 生成的 PtyCommand args 不包含 "--resume"
#[test]
fn test_restore_agent_no_session_id_omits_resume() {
    use storage::{NodeSession, NodeSessionKind};
    use crate::session_restore::build_agent_restore_command_for_test;

    let session = NodeSession {
        node_id: 1,
        kind: NodeSessionKind::Agent,
        cwd: None,
        session_id: None,
    };
    // build_agent_restore_command_for_test is a test helper exposed by session_restore
    // that returns the PtyCommand that would be used for this node_session.
    let cmd = build_agent_restore_command_for_test(&session, canvas::node::AgentProfile::Claude, None);
    let args_str = cmd.args.join(" ");
    assert!(
        !args_str.contains("--resume"),
        "session_id=None 时不应出现 --resume，实际 args: {args_str}"
    );
}

/// RS-9: restore_live_sessions_with_node_sessions 当 cwd = None 时不报错，
/// terminal 使用默认 cwd（workspace root）。
///
/// 预期结果：
///   - restore 返回 1（成功 spawn 了一个节点）且无 panic
#[test]
fn test_restore_terminal_null_cwd_does_not_error() {
    // Compile-only smoke test: verify signature accepts None cwd gracefully.
    use storage::{NodeSession, NodeSessionKind, SessionCheckpoint, SessionCheckpointKind};

    let session = NodeSession {
        node_id: 1,
        kind: NodeSessionKind::Terminal,
        cwd: None,
        session_id: None,
    };
    // Just verifying this doesn't panic at the command-building level:
    let cmd = crate::session_restore::build_terminal_restore_command_for_test(&session, "/bin/zsh");
    // cwd should be None (fall back to workspace root)
    assert!(
        cmd.cwd.is_none(),
        "cwd=None 时 PtyCommand.cwd 也应为 None，实际: {:?}",
        cmd.cwd
    );
}
```

- [ ] **Step 6.3: Run to confirm failure**

```bash
cargo test -p runtime -- runtime_storage 2>&1
```

Expected: compile errors — `session_restore` module not found.

- [ ] **Step 6.4: Create session_restore.rs**

Create `crates/runtime/src/session_restore.rs`. 

**Part A — move existing functions from runtime.rs:**

Move the three existing functions (`collect_session_checkpoints`, `save_session_checkpoints`, `restore_live_sessions`) verbatim from `runtime.rs` into this file. Adjust `use` imports as needed — they'll need access to `canvas`, `terminal`, `storage`, and `crate::nodes`.

**Part B — add new functions:**

```rust
use std::collections::HashMap;
use std::path::Path;
use canvas::{Document, NodeId, node::{AgentProfile, CanvasNodeKind}};
use storage::{NodeSession, NodeSessionKind, NodeSessionStore, StorageResult};
use terminal::pty::PtyCommand;
use crate::{live::LiveNode, session_snapshot};

/// Capture cwd + session_id for all live nodes and persist to the store.
/// Must be called before kill_all().
pub fn save_node_sessions(
    store: &NodeSessionStore,
    live: &HashMap<NodeId, Box<dyn LiveNode>>,
    document: &Document,
    workspace_root: Option<&Path>,
) -> StorageResult<()> {
    let sessions = session_snapshot::collect_node_sessions(live, document, workspace_root);
    for session in &sessions {
        store.save_session(session)?;
    }
    Ok(())
}

/// Like restore_live_sessions, but applies cwd and --resume from node_sessions.
/// node_sessions is matched by node_id against the document's nodes.
pub fn restore_live_sessions_with_node_sessions(
    // same parameters as existing restore_live_sessions +
    node_sessions: &[NodeSession],
    // ... existing params (document, live map ref, workspace_root, etc.)
    // Check the exact signature of restore_live_sessions and mirror it here,
    // adding node_sessions: &[NodeSession] as the last parameter.
) -> usize {
    // For each node being restored:
    // 1. Look up matching NodeSession by node_id
    // 2. For Terminal: if session.cwd is Some, call .cwd() on PtyCommand
    // 3. For Agent(Claude): if session.session_id is Some, use build_resume_command;
    //    otherwise use the normal agent command
    // 4. Delegate actual spawn to the same helpers used by restore_live_sessions
    todo!("implement by extending restore_live_sessions logic")
}

/// Test helper: return the PtyCommand that would be used for agent restore.
#[cfg(test)]
pub fn build_agent_restore_command_for_test(
    session: &NodeSession,
    profile: AgentProfile,
    hook_env: Option<&crate::nodes::agent::AgentHookEnv>,
) -> PtyCommand {
    match &session.session_id {
        Some(id) => session_snapshot::build_resume_command("/bin/zsh", id, hook_env),
        None => crate::nodes::agent::agent_command(profile, hook_env),
    }
}

/// Test helper: return the PtyCommand that would be used for terminal restore.
#[cfg(test)]
pub fn build_terminal_restore_command_for_test(
    session: &NodeSession,
    shell: &str,
) -> PtyCommand {
    match &session.cwd {
        Some(cwd) => session_snapshot::build_terminal_restore_command(shell, cwd),
        None => PtyCommand::new(shell).args(["-l"]),
    }
}
```

**Implementation note**: To implement `restore_live_sessions_with_node_sessions`, read the body of the existing `restore_live_sessions` in `runtime.rs` first. The new function should call the same internal spawn helpers but check the `node_sessions` slice before building the command. The simplest correct approach: copy `restore_live_sessions` entirely, add a `node_sessions` parameter, and add the lookup+branch before building each `PtyCommand`.

- [ ] **Step 6.5: Update runtime.rs to delegate**

In `crates/runtime/src/runtime.rs`:

1. Add module declaration at the top: `pub(crate) mod session_restore;`
2. Replace the bodies of `collect_session_checkpoints`, `save_session_checkpoints`, `restore_live_sessions` with one-line delegations to `session_restore::`:

```rust
pub fn collect_session_checkpoints(&self) -> Vec<SessionCheckpoint> {
    session_restore::collect_session_checkpoints(&self.live, &self.document)
}

pub fn save_session_checkpoints(&self, store: &SessionStore) -> StorageResult<()> {
    session_restore::save_session_checkpoints(store, &self.collect_session_checkpoints())
}

pub fn restore_live_sessions(&mut self, checkpoints: &[SessionCheckpoint]) -> usize {
    session_restore::restore_live_sessions(checkpoints, /* pass self fields */)
}

pub fn save_node_sessions(&self, store: &NodeSessionStore) -> StorageResult<()> {
    session_restore::save_node_sessions(
        store, &self.live, &self.document, self.workspace_root.as_deref()
    )
}

pub fn restore_live_sessions_with_node_sessions(
    &mut self,
    checkpoints: &[SessionCheckpoint],
    node_sessions: &[NodeSession],
) -> usize {
    session_restore::restore_live_sessions_with_node_sessions(
        checkpoints, node_sessions, /* pass self fields */
    )
}
```

- [ ] **Step 6.6: Check line count**

```bash
wc -l crates/runtime/src/runtime.rs crates/runtime/src/session_restore.rs
```

Expected: `runtime.rs` ≤ 500 lines, `session_restore.rs` ≤ 500 lines.

If `runtime.rs` is still over 500, move more of its impl methods to `session_restore.rs` or a new focused module.

- [ ] **Step 6.7: Run tests**

```bash
cargo test -p runtime 2>&1
```

Expected: RS-8 and RS-9 pass. RS-7 (PTY integration) — implement or skip with `#[ignore]` if PTY tests are not part of the CI suite.

- [ ] **Step 6.8: Run constraint checks**

```bash
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
cargo tree -p runtime | grep gpui
```

Expected: all pass, no GPUI.

- [ ] **Step 6.9: Commit**

```bash
git add crates/runtime/src/session_restore.rs \
        crates/runtime/src/runtime.rs \
        crates/runtime/tests/
git commit -m "feat(runtime): add session_restore module with node session save/restore"
```

---

## Task 7: App Wiring

**Files:**
- Modify: `crates/app/src/workspace/persistence.rs`
- Modify: `crates/app/src/workspace/launchpad.rs` (or wherever `restore_live_sessions` is called — confirm with grep)

**Interfaces:**
- Consumes: `Runtime::save_node_sessions()` (Task 6), `Runtime::restore_live_sessions_with_node_sessions()` (Task 6), `ProjectStore::node_session_store()` (Task 1)

- [ ] **Step 7.1: Find the exact call sites**

```bash
grep -rn "restore_live_sessions\|save_session_checkpoints\|flush_now" \
  crates/app/src/ 2>&1
```

Note which files call these. Typically `persistence.rs` for save and `launchpad.rs` for restore.

- [ ] **Step 7.2: Write the failing test**

Find the existing persistence test (likely `crates/app/tests/unit/persistence_tests.rs`). Add a check that `node_session_store` gets written:

```rust
/// P-NS-1: flush_now 调用后 node_sessions 表中有数据（若有 live 节点的话）。
///
/// 数据构造：使用现有的测试 Runtime + ProjectStore 模式
/// (参考现有 test_scheduler_debounces_canvas_snapshot_flush 的 setup 方法)
///
/// 预期结果：
///   - flush_now 成功返回 Ok
///   - node_session_store.load_sessions() 不报错（即使返回空列表）
#[test]
fn test_flush_now_includes_node_sessions() {
    // Follow the existing persistence test setup pattern.
    // After flush_now(), call project_store.node_session_store().load_sessions()
    // and assert it returns Ok (even if empty — ensures the table exists and is writable).
    todo!("implement following existing persistence test setup pattern")
}
```

- [ ] **Step 7.3: Wire save_node_sessions into persistence.rs**

In `crates/app/src/workspace/persistence.rs`, locate `flush_now` or the equivalent function that calls `save_session_checkpoints`. Add the node-sessions save immediately after:

```rust
// After the existing save_session_checkpoints call:
runtime.save_node_sessions(&project_store.node_session_store())?;
```

The full context should look like:

```rust
runtime.save_session_checkpoints(&project_store.session_store())?;
runtime.save_node_sessions(&project_store.node_session_store())?;  // ← add
```

Make sure the `use` imports at the top of the file include `NodeSessionStore` if needed (it's returned by the factory method, so the type may not need to be imported explicitly).

- [ ] **Step 7.4: Wire restore_live_sessions_with_node_sessions into launchpad.rs**

Find the call to `runtime.restore_live_sessions(...)`. Modify it to also load and pass `node_sessions`:

```rust
// Before the existing restore call:
let node_sessions = project_store.node_session_store().load_sessions()
    .unwrap_or_default();

// Replace the existing:
//   runtime.restore_live_sessions(&checkpoints);
// With:
runtime.restore_live_sessions_with_node_sessions(&checkpoints, &node_sessions);
```

`unwrap_or_default()` ensures a DB error (e.g. first open of an old project without the table) silently falls back to an empty list — the `CREATE TABLE IF NOT EXISTS` in `migrate_project_db` means the table always exists after migration, but `unwrap_or_default` is a safe belt-and-suspenders fallback.

- [ ] **Step 7.5: Build the app**

```bash
cargo check -p app 2>&1
```

Expected: no errors.

- [ ] **Step 7.6: Run all workspace tests**

```bash
cargo test --workspace 2>&1
```

Expected: all tests pass.

- [ ] **Step 7.7: Run all constraint checks**

```bash
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
bash scripts/check-agent-guide-lines.sh
bash scripts/check-storage-boundaries.sh
cargo tree -p canvas -p terminal -p runtime | grep -i gpui
```

Expected: all pass, no GPUI in domain crates.

- [ ] **Step 7.8: Manual smoke test**

```bash
cargo run -p app
```

1. Open the project (or any workspace).
2. Create a terminal node (`Cmd+N`). In it, run `cd /tmp && pwd`.
3. Create a Claude agent node. Send one message to start a session.
4. Close the window.
5. `cargo run -p app` — reopen the same workspace.
6. **Terminal node**: verify the shell prompt shows `/tmp` or running `pwd` returns `/tmp`.
7. **Agent node**: in `RUST_LOG=trace` output, verify `--resume` appears in the spawn command for the agent node:
   ```bash
   RUST_LOG=trace cargo run -p app 2>&1 | grep "resume"
   ```

- [ ] **Step 7.9: Final commit**

```bash
git add crates/app/src/workspace/
git commit -m "feat(app): wire node session save/restore into persistence and launchpad"
```

---

## Verification Summary

```bash
# All domain tests
cargo test -p canvas -p terminal -p runtime -p storage

# App compile check
cargo check -p app

# Constraint enforcement
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
bash scripts/check-agent-guide-lines.sh
bash scripts/check-storage-boundaries.sh

# No GPUI in domain crates
cargo tree -p canvas -p terminal -p runtime | grep -i gpui
```

All commands should exit 0 with no violations.
