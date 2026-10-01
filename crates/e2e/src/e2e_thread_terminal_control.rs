use crate::harness::{enter_mobile_canvas, make_app};
use app::{
    MobileAccess, MobileCommand, MobileCommandRequest, MobileServer, ThreadTerminalCommand,
    ThreadTerminalOperation,
};
use gpui::{TestAppContext, VisualTestContext};
use norma::{
    agent_hook::{parse_hook_event, HookSource},
    protocol::{AgentProfile, CardCommand, CardCommandResult},
};
use serde_json::{json, Value};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// 正式持久命令和GPUI前台接管原PTY，拒绝竞争能力并正确读写人工后续回合。
///
/// 数据构造：1个Codex节点、录制PTY、原turn-a与后续turn-human；接管a/b是两个Mac对象。
///   控制手机初始revision=1；接管/一次文字/交还分别使用独立操作ID。
/// 执行过程：接管a→竞争b失败→原PTY文字输入及重复查回执→人工新回合→受控读取→交还→迟到输入。
/// 预期结果：b失败即被撤销，a仍有效；仅一次draft字节到原PTY；后续回合可读取；旧输入不复活。
/// 层级：正式持久命令/前台适配/Runtime/录制PTY，不含真实模型或Computer Use。
#[test]
fn durable_thread_control_uses_original_pty_and_revokes_failed_claim() {
    let writes = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let captured = writes.clone();
    let mut test = TestAppContext::single();
    let (app, cx) = make_app(
        &mut test,
        Box::new(move |_, _, _, tx| {
            Ok(Box::new(terminal::TestPtyBackend::recording(
                tx,
                captured.clone(),
            )))
        }),
    );
    let directory = tempfile::tempdir().expect("原项目");
    let _socket = enter_mobile_canvas(&app, cx, directory.path());
    let CardCommandResult::AgentCreated {
        definition_id,
        node_id: Some(node),
        ..
    } = cx.update(|window, cx| {
        app.update(cx, |app, _| {
            app.execute_agent_card_command_for_e2e(
                window,
                CardCommand::CreateAgent {
                    name: Some("TerminalOwner".into()),
                    profile: AgentProfile::Codex,
                    activate: true,
                },
            )
            .expect("原生Agent")
        })
    })
    else {
        panic!("必须创建唯一原PTY");
    };
    let hook = |event: &str, turn: &str| {
        parse_hook_event(
            HookSource::Codex,
            node,
            json!({"hook_event_name":event,"session_id":"control-session","turn_id":turn}),
        )
        .expect("Provider格式Hook")
    };
    let key = app.update(cx, |app, _| {
        assert!(
            app.runtime
                .observe_mobile_hook(&hook("UserPromptSubmit", "turn-a")),
            "原回合确认"
        );
        assert!(
            app.runtime.observe_mobile_hook(&hook("Stop", "turn-a")),
            "原回合完成"
        );
        app.runtime
            .mobile_latest_turn_key(node)
            .expect("Provider身份")
    });
    let (path, commands, terminals, project, route) = app.read_with(cx, |app, cx| {
        let (project, route) = app.norma_workspace_identity_for_e2e().expect("已注册路由");
        (
            app.app_database_path_for_e2e(),
            app.mobile_command_source_for_e2e(cx),
            app.mobile_terminal_source_for_e2e(cx)
                .expect("正式终端读取通道"),
            project,
            route,
        )
    });
    let store = storage::AppStore::new(path);
    let token = "a".repeat(64); //32字节配对凭据编码为64个hex字符。
    store
        .mobile_access_store()
        .register(
            "phone-terminal",
            "iPhone",
            &token,
            &[storage::MobileProjectAccess {
                workspace_id: project.clone(),
                access: storage::MobilePermission::Control,
            }],
        )
        .expect("项目控制授权");
    let identity = norma::mobile::ServerIdentity::generate().expect("隔离身份");
    let machine = identity.machine_id();
    let server = MobileServer::start_with_sources(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("仅本机")],
        None,
        commands,
    )
    .expect("正式服务");
    let lease = runtime::TerminalInputLease::new(key.clone()).expect("连接a能力");
    let source = app::ThreadTerminalRequest {
        collaboration_workspace_id: "workspace-a".into(),
        thread_id: "thread-a".into(),
        operation_id: "source-task".into(),
        task_ref: "runtime-a:7".into(),
        before: None,
        revision: None,
    };
    let target = app::LiveTerminalTask {
        task_ref: source.task_ref.clone(),
        agent_definition_id: definition_id,
        key: key.clone(),
    };
    let request = |operation_id: &str,
                   lease: Arc<runtime::TerminalInputLease>,
                   operation: ThreadTerminalOperation| MobileCommandRequest {
        operation_id: operation_id.into(),
        workspace_id: project.clone(),
        route_id: route.clone(),
        grant_revision: 1,
        command: MobileCommand::ThreadTerminal {
            command: Box::new(ThreadTerminalCommand {
                origin: source.clone(),
                target: target.clone(),
                operation,
                lease,
            }),
        },
    };
    let claim = request(
        "claim-a",
        lease.clone(),
        ThreadTerminalOperation::Claim {
            operation_id: "claim-a".into(),
        },
    );
    assert_eq!(
        execute(cx, &server, &machine, &token, claim)["receipt"]["result"]["status"],
        "controlled",
        "前台真实接管"
    );
    let competitor = runtime::TerminalInputLease::new(key.clone()).expect("连接b能力");
    let failure = execute(
        cx,
        &server,
        &machine,
        &token,
        request(
            "claim-b",
            competitor.clone(),
            ThreadTerminalOperation::Claim {
                operation_id: "claim-b".into(),
            },
        ),
    );
    assert_eq!(
        failure["receipt"]["error"], "terminal_controlled",
        "原连接独占"
    );
    assert!(
        competitor.is_revoked(),
        "失败接管必须关闭能力，否则该连接后续明确重试会被自己卡住"
    );
    assert!(!lease.is_revoked(), "竞争失败不能撤销合法持有者");
    let scope = norma::mobile::TerminalRequest {
        workspace_id: project.clone(),
        route_id: route.clone(),
        node_id: node,
        run_id: key.run_id.clone(),
        session_id: Some(key.session_id.clone()),
        before: None,
        revision: None,
    };
    let screen = read(cx, &terminals, &scope, &target, &lease).expect("正式读取持有者当前页");
    assert!(screen.input_ready, "持有者当前页可输入");
    let input = request(
        "input-a",
        lease.clone(),
        ThreadTerminalOperation::Input {
            operation_id: "input-a".into(),
            claim_operation_id: "claim-a".into(),
            expected: screen.card,
            screen_signature: screen.screen_signature,
            input_sequence: screen.input_sequence,
            input: norma::mobile::TerminalInput::Text {
                text: "draft".into(),
            },
        },
    );
    let written = execute(cx, &server, &machine, &token, input.clone());
    assert_eq!(
        written["receipt"]["result"]["status"], "written",
        "文字进入原PTY"
    );
    assert_eq!(
        execute(cx, &server, &machine, &token, input),
        written,
        "重复命令只返回原回执"
    );
    app.update(cx, |app, _| {
        assert!(
            app.runtime
                .observe_mobile_hook(&hook("UserPromptSubmit", "turn-human")),
            "人工后续回合"
        );
    });
    let next = read(cx, &terminals, &scope, &target, &lease).expect("原租约继续读取实际回合");
    assert_eq!(
        next.card.turn_id.as_deref(),
        Some("turn-human"),
        "不伪造原turn-a为当前回合"
    );
    let returned = execute(
        cx,
        &server,
        &machine,
        &token,
        request(
            "return-a",
            lease.clone(),
            ThreadTerminalOperation::Release {
                operation_id: "return-a".into(),
                claim_operation_id: "claim-a".into(),
            },
        ),
    );
    assert_eq!(
        returned["receipt"]["result"]["status"], "released",
        "人工后续回合仍可交还"
    );
    assert!(lease.is_revoked(), "交还必须永久关闭能力");
    assert_eq!(
        read(cx, &terminals, &scope, &target, &lease).err(),
        Some("terminal_control_expired"),
        "旧读取不能续期复活"
    );
    assert_eq!(
        *writes.lock().expect("录制字节"),
        vec![b"draft".to_vec()],
        "只有一次人工正文，无自动Enter或清空"
    );
    assert_eq!(
        app.read_with(cx, |app, _| app.runtime.live_len()),
        1,
        "整个流程复用唯一原PTY"
    );
    server.shutdown();
}

fn execute(
    cx: &mut VisualTestContext,
    server: &MobileServer,
    machine: &str,
    token: &str,
    request: MobileCommandRequest,
) -> Value {
    //5s = existing4s foreground scheduling allowance +1s SQLite receipt margin.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let result = server
            .submit_internal_command_for_e2e(machine, "phone-terminal", token, request.clone())
            .expect("内部命令已由网络层完成来源检查，此处验证持久前台路径");
        if result["receipt"]["state"] != "accepted" {
            return result;
        }
        assert!(Instant::now() < deadline, "持久回执不能无限等待");
        cx.executor().advance_clock(Duration::from_millis(20));
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn read(
    cx: &mut VisualTestContext,
    source: &async_channel::Sender<app::LiveTerminalRequest>,
    request: &norma::mobile::TerminalRequest,
    task: &app::LiveTerminalTask,
    lease: &Arc<runtime::TerminalInputLease>,
) -> Result<norma::mobile::TerminalPage, &'static str> {
    let (reply, response) = crossbeam_channel::bounded(1);
    //4s equals production foreground read budget; caller waits at most an additional1s to report failure.
    let deadline = Instant::now() + Duration::from_secs(4);
    source
        .try_send(app::LiveTerminalRequest {
            request: request.clone(),
            task: Some(task.clone()),
            control: Some(lease.clone()),
            deadline,
            reply,
        })
        .expect("正式原终端读取");
    loop {
        cx.executor().advance_clock(Duration::from_millis(20));
        cx.run_until_parked();
        if let Ok(result) = response.try_recv() {
            return result;
        }
        assert!(
            Instant::now() < deadline + Duration::from_secs(1),
            "读取须有界返回"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
