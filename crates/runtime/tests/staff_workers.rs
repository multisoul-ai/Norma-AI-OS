use canvas::{AgentProfile, CanvasRect};
use runtime::{AgentHookRuntimeConfig, Runtime};

/// 幕僚 Worker 总是创建新卡，派发编号随子进程环境下发，旧 hook 不能改变新 Worker。
///
/// 数据构造：两个同名 Simon/Grok，dispatch 为 d-one/d-two；终端 80×24 是标准默认尺寸。
/// 执行过程：1. 先创建两张无进程卡。2. 注入 PTY 工厂并启动第一张。3. 检查启动环境与 hook 过滤。
/// 预期结果：两个 node 不同，启动前无 PTY；环境有本轮 id；仅匹配编号的 hook 被接受。
#[test]
fn worker_launch_identity_is_explicit_and_old_dispatch_is_rejected() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:1".into(),
        token: "test-token".into(),
        workspace_id: "ws".into(),
        workspace_route_id: "route".into(),
        norma_socket: "/tmp/norma-test.sock".into(),
    }));
    let bounds = CanvasRect::new(0., 0., 640., 400.);
    let a = runtime.create_staff_worker(AgentProfile::Grok, "Simon".into(), bounds, "d-one".into());
    let b = runtime.create_staff_worker(AgentProfile::Grok, "Simon".into(), bounds, "d-two".into());
    assert_ne!(
        a, b,
        "each Worker must create its own card despite identical Agent name"
    );
    assert!(
        runtime.agent_child_pid(a).is_none(),
        "binding must be persisted before any process is started"
    );
    let (tx, rx) = std::sync::mpsc::channel();
    runtime.set_pty_factory(Box::new(move |command, _, _, output| {
        tx.send(command).expect("capture launch command");
        Ok(Box::new(terminal::TestPtyBackend::echo(output)))
    }));
    runtime
        .start_native_agent_terminal(a, 80, 24)
        .expect("start staff desktop Worker");
    let command = rx.recv().expect("launch command");
    assert!(
        command
            .env
            .iter()
            .any(|(key, value)| key == "NORMA_DISPATCH_ID" && value == "d-one"),
        "launch must carry exact current dispatch id"
    );
    assert!(
        !command
            .env
            .iter()
            .any(|(key, value)| key == "NORMA_DISPATCH_ID" && value == "d-two"),
        "another Worker id must not leak"
    );
    let hook = |dispatch: &str| {
        norma::agent_hook::parse_hook_event(
            norma::agent_hook::HookSource::Grok,
            a,
            serde_json::json!({"hook_event_name":"Stop","_norma_dispatch_id":dispatch}),
        )
        .expect("hook")
    };
    assert!(
        runtime.accepts_staff_hook(&hook("d-one")),
        "current dispatch hook accepted"
    );
    assert!(
        !runtime.accepts_staff_hook(&hook("old")),
        "late old dispatch hook rejected"
    );
    runtime.remove_node(a);
    assert!(
        runtime.staff_dispatch_id(a).is_none(),
        "removed node must release launch identity"
    );
    assert_eq!(
        runtime.staff_dispatch_id(b),
        Some("d-two"),
        "removing A must preserve B"
    );
    runtime.kill_all();
}
