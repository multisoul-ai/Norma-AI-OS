#![cfg(unix)]
use canvas::{AgentProfile, CanvasRect};
use runtime::{AgentHookRuntimeConfig, AgentTaskState, NodeView, Runtime};
use std::path::Path;
use std::time::{Duration, Instant};

fn card(workspace: &Path) -> (Runtime, canvas::NodeId) {
    let mut runtime = Runtime::new_for_workspace(workspace);
    runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:12345/agent-hooks".into(),
        token: "unused-test-token".into(),
        workspace_id: "test-workspace".into(),
        workspace_route_id: "test-route".into(),
        norma_socket: "/tmp/unused-norma-test.sock".into(),
    }));
    let id = runtime
        .create_mobile_agent_card(
            AgentProfile::Codex,
            None,
            None,
            CanvasRect::new(0.0, 0.0, 640.0, 400.0),
        )
        .expect("应创建卡片");
    (runtime, id)
}

/// Codex 空闲卡片不能靠虚假首条消息初始化；关窗后的后台准备也不能启动。
///
/// 数据构造：一张空卡，标准 80×24 终端；空白任务不代表用户真实指令。
/// 执行过程：预约 → 用空白任务准备 → 失败 → 关闭 → 再使用已失效的预约。
/// 预期结果：无 live、无 session；关闭后的准备失败，不能创建进程或会话。
#[test]
fn idle_codex_requires_a_real_task_and_cannot_launch_after_close() {
    let directory = tempfile::tempdir().expect("应创建测试目录");
    let (mut runtime, id) = card(directory.path());
    let launch = runtime
        .mobile_agent_launch(id, Path::new("unused-hook"), 80, 24)
        .expect("应预约 Codex");
    let generation = launch.generation();
    assert!(
        launch.with_initial_task("  ".into()).prepare().is_err(),
        "空任务不得触发启动"
    );
    assert_eq!(runtime.live_len(), 0, "空卡不得启动终端");
    assert_eq!(runtime.agent_session_id(id), None, "空卡不得绑定假历史");
    runtime.cancel_mobile_agent_launch(id, generation);
    let launch = runtime
        .mobile_agent_launch(id, Path::new("unused-hook"), 80, 24)
        .expect("应可重新预约");
    runtime.kill_all();
    assert!(
        launch
            .with_initial_task("Do not use tools. Reply OK.".into())
            .prepare()
            .is_err(),
        "关窗后迟到的准备不能启动"
    );
}

/// 真实 Codex 服务和 TUI 属于同一卡片，首条任务可读取完整历史，关闭后服务退出。
///
/// 数据构造：独立临时项目与无工具任务；120s = 模型冷启动和生成预算，5s = 本地进程退出预算。
/// 执行过程：后台准备真实指令 → Runtime 附着原生 TUI → 检查启动索引 → 等待完成 → 读历史 → kill_all → 删除卡片。
/// 预期结果：TUI 可见同一回复、历史含精确标记；关闭后旧连接失效，删除后准确索引仍在且不会恢复卡片。
/// 使用真实本机 Codex 与模型；不关闭外部 Hook，不等同于真实 App 或 iPhone。
#[test]
#[ignore = "requires installed Codex and real model access"]
fn controlled_codex_owns_private_server_and_native_terminal() {
    let directory = tempfile::tempdir().expect("应创建独立目录");
    let (mut runtime, id) = card(directory.path());
    let prepared = runtime.mobile_agent_launch(id,Path::new("unused-hook"),100,30)
        .expect("应预约启动")
        .with_initial_task("Do not use any tools or modify files. Reply with exactly NORMA_CONTROLLED_CODEX_READY.".into())
        .with_approval_audit(storage::ApprovalAuditStore::new(directory.path().join("audit.sqlite")))
        .prepare().expect("正式 Codex 应可准备");
    runtime
        .attach_mobile_agent(prepared)
        .expect("应附着本次启动");
    let session = runtime.agent_session_id(id).expect("应绑定精确 session");
    let index = storage::MobileTurnStore::new(directory.path().join("audit.sqlite"));
    let key = storage::MobileSessionKey {
        node_id: id,
        profile: "Codex".into(),
        run_id: runtime.mobile_agent_run_id(id).expect("准确运行").into(),
        session_id: session.clone(),
    };
    let registered = index.session(&key).expect("启动身份查询");
    assert!(
        registered.is_some(),
        "首次任务开始时准确会话身份必须已经落盘"
    );
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut screen = String::new();
    while Instant::now() < deadline {
        runtime.drain_all();
        if let Some(NodeView::Term { snapshot, .. }) = runtime.node_view(id) {
            screen = snapshot.rows.iter().flatten().map(|cell| cell.c).collect();
        }
        if runtime.agent_task_state(id) == Some(AgentTaskState::Done)
            && screen.contains("NORMA_CONTROLLED_CODEX_READY")
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        runtime.agent_task_state(id),
        Some(AgentTaskState::Done),
        "真实 turn/completed 必须推进 Done"
    );
    assert!(
        screen.contains("NORMA_CONTROLLED_CODEX_READY"),
        "原生 TUI 必须附着同一真实会话，当前：{screen}"
    );
    assert!(!screen.contains("no rollout found"), "不得附着空历史");
    let history = runtime
        .mobile_codex_history(id, &session, None)
        .expect("应请求准确会话历史")
        .recv_timeout(Duration::from_secs(5))
        .expect("应收到回执")
        .expect("历史应成功");
    assert!(
        history["data"]
            .as_array()
            .expect("历史应包含回合列表")
            .iter()
            .flat_map(|turn| turn["items"].as_array().into_iter().flatten())
            .any(|item| item["type"] == "agentMessage"
                && item["text"]
                    .as_str()
                    .is_some_and(|text| text.trim() == "NORMA_CONTROLLED_CODEX_READY")),
        "历史必须包含真实回复"
    );
    runtime.kill_all();
    runtime.remove_node(id);
    assert!(
        index.session(&key).expect("删除后索引").is_some(),
        "删除卡片不得丢失准确历史入口"
    );
    assert!(
        runtime.document.node(id).is_none(),
        "读历史索引不能复活已删除卡片"
    );
    assert!(
        runtime.mobile_codex_history(id, &session, None).is_err(),
        "关窗后旧控制连接不可用"
    );
}
