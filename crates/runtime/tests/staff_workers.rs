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

/// 场景：兼容旧 helper 时仍拒绝缺身份、其他进程和显式旧派发编号。
/// 数据构造：1 张 Codex Worker；80×24 为默认终端尺寸；当前进程由 Runtime 生成。
/// 执行过程：创建并预备 Worker，分别检查当前 run、旧 run、无 run、错误/畸形 dispatch。
/// 预期结果：只有当前进程且缺 dispatch 的旧载荷可接受；所有身份冲突均拒绝。
#[test]
fn legacy_staff_hook_requires_exact_current_process_identity() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    let node = runtime.create_staff_worker(
        AgentProfile::Codex,
        "Alan".into(),
        CanvasRect::new(0., 0., 640., 400.),
        "current-dispatch".into(),
    );
    let launch = runtime
        .prepare_native_agent_terminal(node, 80, 24)
        .expect("预备进程，不启动外部 CLI");
    let hook = |raw| {
        norma::agent_hook::parse_hook_event(norma::agent_hook::HookSource::Codex, node, raw)
            .expect("SessionStart 载荷")
    };
    let current = hook(
        serde_json::json!({"hook_event_name":"SessionStart", "_norma_control_run_id":launch.run_id()}),
    );
    let old = hook(
        serde_json::json!({"hook_event_name":"SessionStart", "_norma_control_run_id":"retired-process"}),
    );
    let missing = hook(serde_json::json!({"hook_event_name":"SessionStart"}));
    let conflict = hook(
        serde_json::json!({"hook_event_name":"SessionStart", "_norma_control_run_id":launch.run_id(), "_norma_dispatch_id":"old-dispatch"}),
    );
    let malformed = hook(
        serde_json::json!({"hook_event_name":"SessionStart", "_norma_control_run_id":launch.run_id(), "_norma_dispatch_id":null}),
    );
    assert!(
        runtime.accepts_staff_hook(&current),
        "旧 helper 的当前进程身份足以关联本次派发"
    );
    assert!(
        !runtime.accepts_staff_hook(&old),
        "旧进程迟到消息不得唤醒新 Worker"
    );
    assert!(
        !runtime.accepts_staff_hook(&missing),
        "只有 node 的消息不能冒认当前派发"
    );
    assert!(
        !runtime.accepts_staff_hook(&conflict),
        "显式旧 dispatch 不能靠当前 run 绕过"
    );
    assert!(
        !runtime.accepts_staff_hook(&malformed),
        "畸形 dispatch 不能按旧 helper 处理"
    );
    runtime.kill_all();
}

/// 幕僚指派选定的模型必须进入该 Worker 的启动命令，且不影响其他卡片。
///
/// 数据构造：选了 `opus` 的 Claude 卡、未选模型的 Codex 卡、选了模型的 Hermes 卡（无已验证的 --model）。
/// 执行过程：分别 prepare，读取启动脚本。
/// 预期结果：Claude 脚本以 `--model 'opus'` 结尾；Codex 不含 `--model`；Hermes prepare 失败。
#[test]
fn staff_worker_model_reaches_launch_command() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    let bounds = CanvasRect::new(0., 0., 640., 400.);
    let claude = runtime.create_staff_worker(AgentProfile::Claude, "A".into(), bounds, "d1".into());
    runtime.set_staff_model(claude, "opus".into());
    let codex = runtime.create_staff_worker(AgentProfile::Codex, "B".into(), bounds, "d2".into());
    let hermes = runtime.create_staff_worker(AgentProfile::Hermes, "C".into(), bounds, "d3".into());
    runtime.set_staff_model(hermes, "x".into());
    let with_model = runtime
        .prepare_native_agent_terminal(claude, 80, 24)
        .expect("claude")
        .command()
        .args
        .last()
        .cloned()
        .unwrap_or_default();
    let without = runtime
        .prepare_native_agent_terminal(codex, 80, 24)
        .expect("codex")
        .command()
        .args
        .last()
        .cloned()
        .unwrap_or_default();
    assert!(
        with_model.ends_with("--model 'opus'"),
        "实际脚本：{with_model}"
    );
    assert!(!without.contains("--model"), "实际脚本：{without}");
    assert!(runtime
        .prepare_native_agent_terminal(hermes, 80, 24)
        .is_err());
}
