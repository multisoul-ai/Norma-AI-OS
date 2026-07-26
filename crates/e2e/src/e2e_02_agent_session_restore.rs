//! Agent 会话恢复 L1：旧库缺失 session id 时，打开工作空间仍应恢复 Provider 历史会话。

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use canvas::{
    AgentProfile, AgentSpec, CanvasNodeKind, CanvasNodeSnapshot, CanvasRect, DocumentSnapshot,
    Viewport,
};
use gpui::TestAppContext;
use storage::{NodeSession, NodeSessionKind, SessionCheckpoint, SessionCheckpointKind};

use crate::harness::{drain_until, make_app_real, project_store};

static PROCESS_ENV_LOCK: Mutex<()> = Mutex::new(());

struct ProcessEnvGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    home: Option<std::ffi::OsString>,
    path: Option<std::ffi::OsString>,
}

impl ProcessEnvGuard {
    fn set(home: &Path, bin: &Path) -> Self {
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
            std::env::join_paths(paths).expect("测试 fake bin 与原 PATH 应能组合成合法 PATH");
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

/// 旧项目数据库没有 hook session id 时，打开工作空间应发现 Claude 历史并恢复原会话。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent node_id = 7，profile = Claude，checkpoint grid = 80×24
///   node_sessions[node_id=7].session_id = None，模拟回归前旧库或 Hook 漏报
///   Claude history = `.claude/projects/<encoded-root>/claude-history-session.jsonl`
///   fake Claude 输出 = `RESTORED_SESSION:<第三个参数>`；resume 命令第三个参数应为 session id
///   timeout = 2s；GPUI 每轮推进 12ms，理论最多约 2000/12 ≈ 166 轮
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 写入包含 Claude Agent 卡片的 canvas snapshot、checkpoint 和空 session id
///   2. 在临时 HOME 创建匹配工作空间的 Claude 历史文件，并用 fake CLI 捕获启动参数
///   3. 通过 App 的真实 workspace open 编排加载卡片和持久化记录
///   4. 延迟恢复在后台发现历史 session，前台生成 `claude --resume` 并启动 fake CLI
///   5. 推进 GPUI drain，读取 Agent 卡片 grid 中的 fake CLI 输出
///
/// 预期结果：
///   - 正断言：卡片输出包含 `RESTORED_SESSION:claude-history-session`
///   - 正断言：Agent live node 成功恢复，不停留在静态 snapshot
///   - 负断言：输出不应是 `RESTORED_SESSION:` 空值，否则仍启动了全新 Session
#[test]
fn e2e_02_workspace_open_restores_agent_session_from_provider_history() {
    let temp = tempfile::tempdir().expect("Agent session restore E2E 应创建临时根目录");
    let workspace = temp.path().join("workspace");
    let home = temp.path().join("home");
    let bin = temp.path().join("bin");
    std::fs::create_dir_all(&workspace).expect("E2E workspace 目录应创建成功");
    std::fs::create_dir_all(&home).expect("E2E HOME 目录应创建成功");
    std::fs::create_dir_all(&bin).expect("E2E fake bin 目录应创建成功");
    let workspace = workspace
        .canonicalize()
        .expect("E2E workspace 应规范化为与 App 打开流程相同的 root");
    prepare_claude_history(&workspace, &home);
    prepare_fake_claude(&bin);
    let _env = ProcessEnvGuard::set(&home, &bin);

    let mut cx = TestAppContext::single();
    let (app, cx) = make_app_real(&mut cx);
    prepare_persisted_agent(project_store(&app, cx, &workspace));
    app.update(cx, |app, cx| {
        app.open_workspace_for_e2e(&workspace, cx)
            .expect("E2E 应通过真实 workspace open 编排打开项目");
    });

    let restored_text = drain_until(
        &app,
        cx,
        |runtime| {
            let runtime::NodeView::Term { snapshot, .. } = runtime.node_view(7)?;
            let text: String = snapshot.rows.iter().flatten().map(|cell| cell.c).collect();
            text.contains("RESTORED_SESSION:").then_some(text)
        },
        Duration::from_secs(2),
    )
    .expect("2 秒内应启动 fake Claude 并在 Agent 卡片 grid 输出恢复的 session id");

    assert!(
        restored_text.contains("RESTORED_SESSION:claude-history-session"),
        "Workspace 打开后 Agent 卡片应恢复 Provider 历史 session，实际 grid: {restored_text:?}"
    );
    assert!(
        app.read_with(cx, |app, _cx| app.runtime.node_view(7).is_some()),
        "node_id=7 的 Agent live node 应完成恢复，不应只剩静态 canvas snapshot"
    );
    assert!(
        restored_text.trim() != "RESTORED_SESSION:",
        "fake Claude 不应收到空 session id，否则说明恢复路径仍启动了全新 Session，实际 grid: {restored_text:?}"
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}

fn prepare_persisted_agent(store: storage::ProjectStore) {
    store.migrate().expect("E2E project schema 应迁移成功");
    store
        .canvas_store()
        .save_snapshot(&DocumentSnapshot {
            nodes: vec![CanvasNodeSnapshot {
                id: 7,
                kind: CanvasNodeKind::Agent(AgentSpec {
                    profile: AgentProfile::Claude,
                    alias: Some("Raya".to_string()),
                    definition_id: None,
                }),
                bounds: CanvasRect::new(10.0, 20.0, 640.0, 400.0),
                z_index: 1,
            }],
            viewport: Viewport::default(),
        })
        .expect("Agent canvas snapshot 应写入成功");
    store
        .session_store()
        .save_checkpoint(&SessionCheckpoint {
            node_id: 7,
            kind: SessionCheckpointKind::Agent,
            payload: "Raya".to_string(),
            cols: 80,
            rows: 24,
            shell: None,
            agent_profile: Some(AgentProfile::Claude),
        })
        .expect("Agent session checkpoint 应写入成功");
    store
        .node_session_store()
        .save_session(&NodeSession {
            node_id: 7,
            kind: NodeSessionKind::Agent,
            cwd: None,
            session_id: None,
        })
        .expect("空 session id 的旧 Agent node session 应写入成功");
}

fn prepare_claude_history(workspace: &Path, home: &Path) {
    let encoded = workspace.to_string_lossy().replace('/', "-");
    let history_dir = home.join(".claude").join("projects").join(encoded);
    std::fs::create_dir_all(&history_dir).expect("Claude history 目录应创建成功");
    std::fs::write(history_dir.join("claude-history-session.jsonl"), b"{}\n")
        .expect("Claude history session 文件应写入成功");
}

fn prepare_fake_claude(bin: &Path) {
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
