//! Runtime + real SQLite integration. No GUI or real Provider process is started.
use canvas::{AgentProfile, CanvasNodeKind, CanvasRect};
use runtime::{NodeError, NodeInput, Runtime};
use storage::CanvasStore;

/// 手机卡片创建、存储、重开和删除形成元数据闭环，输入不得隐式启动任务。
///
/// 数据构造：临时 SQLite 保存同名原生 Codex / 受控 Claude 卡片各一张；两者拥有不同节点 ID。
/// 640×400 是画布占位尺寸，不代表设备屏幕或 token 预算。
///
/// 执行过程：创建两卡并将Codex标为原生模式 → 保存 → 新 Runtime 读库 → 尝试向空闲卡片输入
/// → 拒绝旧名称快照、接受准确原生快照 → 删除 Codex
/// → 再次保存和加载 → 检查 Claude 保留及 Codex 不存在。
///
/// 预期结果：各自运行模式、身份与名称往返；未安排任务不能输入；准确删除原生卡，另一卡保留。
#[test]
fn mobile_card_metadata_round_trip_and_delete_do_not_dispatch() {
    let directory = tempfile::tempdir().expect("应创建独立测试目录");
    let store = CanvasStore::new(directory.path().join("project.sqlite"));
    let mut first = Runtime::new_for_workspace(directory.path());
    let bounds = CanvasRect::new(0.0, 0.0, 640.0, 400.0);
    let codex = first
        .create_mobile_agent_card(AgentProfile::Codex, None, Some("开发".into()), bounds)
        .expect("Codex 空闲卡片应创建成功");
    let claude = first
        .create_mobile_agent_card(AgentProfile::Claude, None, Some("开发".into()), bounds)
        .expect("Claude 同名卡片应具有独立身份");
    if let CanvasNodeKind::Agent(spec) = &mut first.document.node_mut(codex).expect("旧卡").kind {
        spec.mobile_control = false;
    }
    first
        .save_canvas_snapshot(&store)
        .expect("两张卡片应保存至 SQLite");
    let mut reopened = Runtime::new_for_workspace(directory.path());
    assert!(
        reopened
            .load_canvas_snapshot(&store)
            .expect("应可读回 SQLite"),
        "已保存的快照必须存在"
    );
    assert_ne!(codex, claude, "同名卡片不得合并身份");
    assert!(
        matches!(&reopened.document.node(codex).expect("Codex 应保留").kind,
        CanvasNodeKind::Agent(spec) if !spec.mobile_control && spec.profile == AgentProfile::Codex),
        "Codex 原生模式与 Provider 必须往返"
    );
    assert!(
        matches!(
            reopened.send_node_input(codex, NodeInput::Text("安排任务".into())),
            Err(NodeError::MissingNode)
        ),
        "普通终端输入不得绕过显式任务派发来启动空闲卡片"
    );
    assert_eq!(
        reopened.live_len(),
        0,
        "创建、保存和输入失败之后均不得存在执行器"
    );
    let expected = reopened
        .mobile_card_snapshots()
        .into_iter()
        .find(|card| card.node_id == codex)
        .expect("原生卡快照");
    let mut stale = expected.clone();
    stale.title = "旧名称".into();
    assert_eq!(
        reopened.validate_mobile_card_deletion(&stale, false),
        Err("card_changed"),
        "旧名称不能确认删除"
    );
    assert_eq!(
        reopened.validate_mobile_card_deletion(&expected, false),
        Ok(()),
        "正式删除校验必须接受原生空卡"
    );
    assert!(reopened.remove_node(codex), "应能删除指定空闲 Codex 卡片");
    reopened
        .save_canvas_snapshot(&store)
        .expect("应持久化删除结果");
    let mut after_delete = Runtime::new();
    assert!(
        after_delete
            .load_canvas_snapshot(&store)
            .expect("删除后仍应有快照"),
        "快照不应被整份删除"
    );
    assert!(
        after_delete.document.node(codex).is_none(),
        "Codex 被删后不得在重开时复活"
    );
    assert!(
        matches!(&after_delete.document.node(claude).expect("另一张 Claude 必须保留").kind,
        CanvasNodeKind::Agent(spec) if spec.mobile_control && spec.alias.as_deref() == Some("开发")),
        "删除同名 Codex 不得影响 Claude 名称或控制模式"
    );
    assert_eq!(after_delete.live_len(), 0, "删除后的恢复不得安排任何任务");
}

/// 手机创建的 Codex / Claude 卡片在首次启动和原会话重开时均使用完全访问。
///
/// 数据构造：2 张手机卡片各启动 2 次，共捕获 4 条正式 PTY 命令；
///   640×400 是画布占位，80×24 是现有默认终端网格；echo 工厂不调用真实 Provider。
/// 执行过程：通过手机建卡入口启动两卡 → 重载原画布快照 → 读回精确会话绑定 → 原卡恢复。
/// 预期结果：两种 Provider 的新建和恢复都带完全访问参数，无逐次审批参数；
///   恢复使用各自保存的会话，保留项目目录和 Hook，并且不创建额外卡片。
#[test]
fn mobile_card_creation_and_resume_default_to_full_access() {
    let directory = tempfile::tempdir().expect("隔离项目目录");
    let mut runtime = Runtime::new_for_workspace(directory.path());
    runtime.set_agent_hook_config(Some(runtime::AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:12345/agent-hooks".into(),
        token: "test-token".into(),
        workspace_id: "workspace-a".into(),
        workspace_route_id: "route-a".into(),
        norma_socket: "/tmp/norma-test.sock".into(),
    }));
    let (sender, commands) = std::sync::mpsc::channel();
    runtime.set_pty_factory(Box::new(move |command, _, _, tx| {
        sender.send(command).expect("记录正式启动参数");
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let bounds = CanvasRect::new(0.0, 0.0, 640.0, 400.0);
    let codex = runtime
        .spawn_mobile_agent_card(AgentProfile::Codex, None, bounds, 80, 24)
        .expect("手机创建 Codex");
    let claude = runtime
        .spawn_mobile_agent_card(AgentProfile::Claude, None, bounds, 80, 24)
        .expect("手机创建 Claude");
    let codex_new = commands.try_recv().expect("Codex 新建命令");
    let claude_new = commands.try_recv().expect("Claude 新建命令");
    assert!(
        codex_new.args[1].contains("--dangerously-bypass-approvals-and-sandbox"),
        "手机新建 Codex 必须默认完全访问"
    );
    assert!(
        !codex_new.args[1].contains("--sandbox")
            && !codex_new.args[1].contains("--ask-for-approval"),
        "不能残留只读或逐次审批参数"
    );
    assert!(
        claude_new.args[1].contains("--dangerously-skip-permissions"),
        "手机新建 Claude 必须默认完全访问"
    );
    assert!(
        !claude_new.args[1].contains("--permission-mode default"),
        "Claude 不能保留逐次审批模式"
    );
    assert!(
        codex_new.args[1].contains("--dangerously-bypass-hook-trust"),
        "Codex Hook 仍应可用"
    );
    assert_eq!(
        codex_new.cwd.as_deref(),
        Some(directory.path()),
        "Codex 必须在目标项目启动"
    );
    assert_eq!(
        claude_new.cwd.as_deref(),
        Some(directory.path()),
        "Claude 必须在目标项目启动"
    );
    runtime.restore_canvas_snapshot(runtime.collect_canvas_snapshot());
    runtime.hydrate_agent_session_bindings(&[
        storage::NodeSession {
            node_id: codex,
            kind: storage::NodeSessionKind::Agent,
            cwd: None,
            session_id: Some("codex-saved".into()),
        },
        storage::NodeSession {
            node_id: claude,
            kind: storage::NodeSessionKind::Agent,
            cwd: None,
            session_id: Some("claude-saved".into()),
        },
    ]);
    assert_eq!(runtime.live_len(), 0, "重开前旧执行器必须已停止");
    runtime
        .start_native_agent_terminal(codex, 80, 24)
        .expect("恢复 Codex 原卡");
    runtime
        .start_native_agent_terminal(claude, 80, 24)
        .expect("恢复 Claude 原卡");
    let codex_resume = commands.try_recv().expect("Codex 恢复命令");
    let claude_resume = commands.try_recv().expect("Claude 恢复命令");
    assert!(
        codex_resume.args[1].contains("--dangerously-bypass-approvals-and-sandbox"),
        "Codex 恢复仍应完全访问"
    );
    assert!(
        !codex_resume.args[1].contains("--sandbox")
            && !codex_resume.args[1].contains("--ask-for-approval"),
        "Codex 恢复不能重新限制权限"
    );
    assert!(
        claude_resume.args[1].contains("--dangerously-skip-permissions"),
        "Claude 恢复仍应完全访问"
    );
    assert!(
        !claude_resume.args[1].contains("--permission-mode default"),
        "Claude 恢复不能重新要求逐次审批"
    );
    assert!(
        codex_resume.args[1].contains("resume 'codex-saved'"),
        "Codex 必须恢复自己的会话"
    );
    assert!(
        claude_resume.args[1].contains("--resume 'claude-saved'"),
        "Claude 必须恢复自己的会话"
    );
    assert!(commands.try_recv().is_err(), "新建和恢复不得启动额外 PTY");
    assert_eq!(runtime.document.nodes().len(), 2, "恢复不能创建重复卡片");
    assert_eq!(runtime.live_len(), 2, "两张原卡均应恢复执行器");
}
