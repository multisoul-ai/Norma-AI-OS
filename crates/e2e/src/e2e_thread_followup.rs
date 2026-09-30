use crate::harness::{enter_mobile_canvas, make_app};
use app::{MobileAccess, MobileCommand, MobileCommandRequest, MobileServer};
use gpui::TestAppContext;
use norma::{
    agent_hook::{parse_hook_event, HookSource},
    protocol::{AgentProfile, CardCommand, CardCommandResult},
};
use serde_json::json;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// 追问排队后节点被新回合复用，必须在真实前台写入前拒绝旧锚点。
/// 数据构造：1个注册Codex/录制PTY；原turn-a、当前turn-b；错误正文must-not-send共13字节。
/// 执行过程：SessionStart→确认a→确认b→用当前快照但旧锚点提交→通过既有fixture注册新协调任务但不确认新回合→再次追问。
/// 预期结果：旧锚点明确session_changed且PTY没有任何正文/Enter；节点没有重建。
///   2s=既有前台4s预算的一半，足够覆盖本机空项目baseline和100ms paste/Enter间隔。
/// 层级：正式持久命令/GPUI前台/Runtime/录制PTY，不等于Computer Use或真实模型。
#[test]
fn followup_rechecks_original_turn_before_any_pty_input() {
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
    let directory = tempfile::tempdir().expect("隔离原项目");
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
                    name: Some("FollowupOwner".into()),
                    profile: AgentProfile::Codex,
                    activate: true,
                },
            )
            .expect("原生Agent")
        })
    })
    else {
        panic!("原Agent必须有唯一PTY");
    };
    let hook = |event: &str, turn: &str| {
        parse_hook_event(
            HookSource::Codex,
            node,
            json!({"hook_event_name":event,"session_id":"followup-session","turn_id":turn}),
        )
        .expect("Provider格式Hook")
    };
    app.update(cx, |app, _| {
        app.enqueue_agent_hook_event_for_e2e(hook("SessionStart", "turn-a"))
            .expect("正常启动Hook")
    });
    crate::harness::drain_until(
        &app,
        cx,
        |runtime| {
            (runtime.agent_task_state(node) == Some(runtime::AgentTaskState::Ready)).then_some(())
        },
        Duration::from_secs(2),
    )
    .expect("原PTY就绪");
    let key = app.update(cx, |app, _| {
        assert!(
            app.runtime
                .observe_mobile_hook(&hook("UserPromptSubmit", "turn-a")),
            "原Provider回合已确认"
        );
        assert!(
            app.runtime.observe_mobile_hook(&hook("Stop", "turn-a")),
            "原回合结束"
        );
        app.runtime
            .mobile_latest_turn_key(node)
            .expect("原回合锚点")
    });
    app.update(cx, |app, _| {
        assert!(
            app.runtime
                .observe_mobile_hook(&hook("UserPromptSubmit", "turn-b")),
            "同节点已有新回合"
        );
        assert!(
            app.runtime.observe_mobile_hook(&hook("Stop", "turn-b")),
            "新回合已结束，不能仅靠busy状态拦截"
        );
    });
    let (path, commands, project, route, current) = app.read_with(cx, |app, cx| {
        let (project, route) = app.norma_workspace_identity_for_e2e().expect("项目路由");
        (
            app.app_database_path_for_e2e(),
            app.mobile_command_source_for_e2e(cx),
            project,
            route,
            app.runtime
                .mobile_card_snapshots()
                .into_iter()
                .find(|card| card.node_id == node)
                .expect("当前卡片"),
        )
    });
    let store = storage::AppStore::new(path);
    let token = "a".repeat(64); //32字节隔离配对凭据×2个hex字符=64。
    store
        .mobile_access_store()
        .register(
            "phone-followup",
            "iPhone",
            &token,
            &[storage::MobileProjectAccess {
                workspace_id: project.clone(),
                access: storage::MobilePermission::Control,
            }],
        )
        .expect("明确项目授权");
    let identity = norma::mobile::ServerIdentity::generate().expect("隔离Mac身份");
    let machine = identity.machine_id();
    let server = MobileServer::start_with_sources(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("仅本机")],
        None,
        commands,
    )
    .expect("正式命令服务");
    let request = MobileCommandRequest {
        operation_id: "stale-followup".into(),
        workspace_id: project,
        route_id: route,
        grant_revision: 1,
        command: MobileCommand::ThreadFollowup {
            target: app::LiveTerminalTask {
                task_ref: "original-runtime:7".into(),
                agent_definition_id: definition_id,
                key,
            },
            expected: current,
            text: "must-not-send".into(),
        },
    };
    let accepted = server
        .submit_internal_command_for_e2e(&machine, "phone-followup", &token, request.clone())
        .expect("进入真实持久命令");
    assert_eq!(
        accepted["receipt"]["state"], "accepted",
        "持久接收不代表已向PTY输入"
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    let receipt = loop {
        cx.executor().advance_clock(Duration::from_millis(20));
        cx.run_until_parked();
        assert!(
            writes.lock().expect("记录输入").is_empty(),
            "原任务锚点失效时，正文或Enter均不得写入被复用的PTY"
        );
        let result = server
            .submit_internal_command_for_e2e(&machine, "phone-followup", &token, request.clone())
            .expect("只读原回执");
        if result["receipt"]["state"] != "accepted" {
            break result;
        }
        assert!(
            Instant::now() < deadline,
            "失效任务须在2秒前台预算内明确拒绝"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(
        receipt["receipt"]["state"], "rejected",
        "前台预检失效不应报结果未知"
    );
    assert_eq!(
        receipt["receipt"]["error"], "session_changed",
        "必须说明原任务回合已经变化"
    );
    assert!(
        receipt["receipt"]["result"].is_null(),
        "拒绝不能伪造Provider新回合"
    );
    assert_eq!(
        app.read_with(cx, |app, _| app.runtime.live_len()),
        1,
        "不得另建Agent或终端"
    );
    let current_key = app.read_with(cx, |app, _| {
        app.runtime.mobile_latest_turn_key(node).expect("当前b回合")
    });
    app.update(cx, |app, _| {
        assert!(
            app.register_external_agent_root_for_e2e(node, Some("替代协调任务".into())),
            "使用既有协调任务fixture建立新归属，不等待Provider新回合"
        );
    });
    assert_eq!(
        app.read_with(cx, |app, _| app.runtime.mobile_latest_turn_key(node)),
        Some(current_key.clone()),
        "任务替换先于新Provider Hook，单查turn不足以阻止误投"
    );
    let mut replaced = request;
    replaced.operation_id = "replaced-followup".into();
    if let MobileCommand::ThreadFollowup {
        target, expected, ..
    } = &mut replaced.command
    {
        target.key = current_key;
        *expected = app.read_with(cx, |app, _| {
            app.runtime
                .mobile_card_snapshots()
                .into_iter()
                .find(|card| card.node_id == node)
                .expect("新任务当前快照")
        });
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    let rejected = loop {
        let result = server
            .submit_internal_command_for_e2e(&machine, "phone-followup", &token, replaced.clone())
            .expect("核对原操作");
        if result["receipt"]["state"] != "accepted" {
            break result;
        }
        assert!(Instant::now() < deadline, "任务归属变化必须立即拒绝");
        cx.executor().advance_clock(Duration::from_millis(20));
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(
        rejected["receipt"]["error"], "session_changed",
        "相同Provider turn也不能越过完整task_ref归属"
    );
    assert!(
        writes
            .lock()
            .expect("PTY记录")
            .iter()
            .all(|bytes| !String::from_utf8_lossy(bytes).contains("must-not-send")),
        "允许的新任务字节不能夹带原追问"
    );
    server.shutdown();
}
