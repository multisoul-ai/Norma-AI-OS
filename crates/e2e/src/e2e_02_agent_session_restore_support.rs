use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Mutex;

use canvas::{
    AgentProfile, AgentSpec, CanvasNodeKind, CanvasNodeSnapshot, CanvasRect, DocumentSnapshot,
    Viewport,
};
use storage::{NodeSession, NodeSessionKind, SessionCheckpoint, SessionCheckpointKind};

static PROCESS_ENV_LOCK: Mutex<()> = Mutex::new(());

pub(super) struct ProcessEnvGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    home: Option<std::ffi::OsString>,
    path: Option<std::ffi::OsString>,
}

impl ProcessEnvGuard {
    pub(super) fn set(home: &Path, bin: &Path) -> Self {
        let lock = PROCESS_ENV_LOCK
            .lock()
            .expect("Agent session restore 环境变量测试锁不应中毒");
        let previous_home = std::env::var_os("HOME");
        let previous_path = std::env::var_os("PATH");
        let mut paths = vec![bin.to_path_buf()];
        if let Some(existing) = previous_path.as_deref() {
            paths.extend(std::env::split_paths(existing));
        }
        let joined_path =
            std::env::join_paths(paths).expect("测试 fake bin 与原 PATH 应能组成合法 PATH");
        std::env::set_var("HOME", home);
        std::env::set_var("PATH", joined_path);
        Self {
            _lock: lock,
            home: previous_home,
            path: previous_path,
        }
    }
}

impl Drop for ProcessEnvGuard {
    fn drop(&mut self) {
        restore_env("HOME", self.home.take());
        restore_env("PATH", self.path.take());
    }
}

fn restore_env(key: &str, value: Option<std::ffi::OsString>) {
    match value {
        Some(value) => std::env::set_var(key, value),
        None => std::env::remove_var(key),
    }
}

pub(super) fn prepare_persisted_agent(store: storage::ProjectStore) {
    store.migrate().expect("E2E project schema 应迁移成功");
    store
        .canvas_store()
        .save_snapshot(&DocumentSnapshot {
            nodes: vec![agent_node(7, "Raya")],
            viewport: Viewport::default(),
        })
        .expect("Agent canvas snapshot 应写入成功");
    save_agent_checkpoint(&store, 7, "Raya");
    save_agent_session(&store, 7, None);
}

pub(super) fn prepare_multi_persisted_agents(
    store: storage::ProjectStore,
    session_ids: [Option<&str>; 2],
) {
    store
        .migrate()
        .expect("多 Agent E2E project schema 应迁移成功");
    store
        .canvas_store()
        .save_snapshot(&DocumentSnapshot {
            nodes: vec![agent_node(7, "Agent-7"), agent_node(8, "Agent-8")],
            viewport: Viewport::default(),
        })
        .expect("多 Agent canvas snapshot 应写入成功");
    for (node_id, session_id) in [7, 8].into_iter().zip(session_ids) {
        save_agent_checkpoint(&store, node_id, &format!("Agent-{node_id}"));
        save_agent_session(&store, node_id, session_id);
    }
}

fn agent_node(node_id: u64, alias: &str) -> CanvasNodeSnapshot {
    CanvasNodeSnapshot {
        id: node_id,
        kind: CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Claude,
            alias: Some(alias.to_string()),
            definition_id: None,
            mobile_control: false,
        }),
        bounds: CanvasRect::new(10.0, 20.0, 640.0, 400.0),
        z_index: node_id as i32,
    }
}

fn save_agent_checkpoint(store: &storage::ProjectStore, node_id: u64, payload: &str) {
    store
        .session_store()
        .save_checkpoint(&SessionCheckpoint {
            node_id,
            kind: SessionCheckpointKind::Agent,
            payload: payload.to_string(),
            cols: 80,
            rows: 24,
            shell: None,
            agent_profile: Some(AgentProfile::Claude),
        })
        .expect("Agent checkpoint 应写入成功");
}

fn save_agent_session(store: &storage::ProjectStore, node_id: u64, session_id: Option<&str>) {
    store
        .node_session_store()
        .save_session(&NodeSession {
            node_id,
            kind: NodeSessionKind::Agent,
            cwd: None,
            session_id: session_id.map(ToString::to_string),
        })
        .expect("Agent node session 应写入成功");
}

pub(super) fn prepare_claude_history(workspace: &Path, home: &Path) {
    let encoded = workspace.to_string_lossy().replace('/', "-");
    let history_dir = home.join(".claude").join("projects").join(encoded);
    std::fs::create_dir_all(&history_dir).expect("Claude history 目录应创建成功");
    std::fs::write(history_dir.join("claude-history-session.jsonl"), b"{}\n")
        .expect("Claude history session 文件应写入成功");
}

pub(super) fn prepare_fake_claude(bin: &Path) {
    let executable = bin.join("claude");
    std::fs::write(
        &executable,
        b"#!/bin/sh\nprintf 'RESTORED_SESSION:%s\\n' \"$3\"\nsleep 30\n",
    )
    .expect("fake Claude 脚本应写入成功");
    let mut permissions = std::fs::metadata(&executable)
        .expect("fake Claude metadata 应可读取")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&executable, permissions).expect("fake Claude 脚本应设置为可执行");
}
