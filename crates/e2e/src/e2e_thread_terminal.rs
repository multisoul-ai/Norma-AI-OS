use crate::harness::{enter_mobile_canvas, make_app};
use app::{LiveTerminalRequest, LiveTerminalTask};
use gpui::TestAppContext;
use norma::protocol::{AgentProfile, CardCommand, CardCommandResult};
use norma::{
    agent_hook::{parse_hook_event, HookSource},
    mobile::TerminalRequest,
};
use serde_json::json;
use std::time::{Duration, Instant};

/// 接管中的自动派工和会话追问保留在既有队列，交还或后台撤权后仍只提交一次。
///
/// 数据构造：一个原生Codex卡、录制PTY；任务"queued-task"和追问"queued-reply"，没有真实模型。
///   500ms = 100ms paste/Enter间隔 + 400ms Hook/前台调度余量；1s观察覆盖两条后台推进路径。
/// 执行过程：确认原session/turn→接管→正式派工→观察无字节→交还→确认派工一次；
///   Provider确认完成→再次接管→正式追问→观察无字节→后台撤权→确认追问一次。
/// 预期结果：接管期间回执为Queued而非Failed/Sent；交还后原队列继续，原PTY中每份正文和Enter只出现一次。
/// 层级：正式App命令与headless GPUI调度/录制PTY，不是Computer Use或真实Coding Agent验收。
#[test]
fn terminal_control_retains_dispatch_and_reply_until_return_or_revoke() {
    use norma::protocol::{AgentTaskAssignment, AgentTaskReceiptStatus};
    use std::sync::{Arc, Mutex};
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
    let workspace = tempfile::tempdir().expect("隔离项目");
    let _socket = enter_mobile_canvas(&app, cx, workspace.path());
    let created = cx.update(|window, cx| {
        app.update(cx, |app, _| {
            app.execute_agent_card_command_for_e2e(
                window,
                CardCommand::CreateAgent {
                    name: Some("ControlProbe".into()),
                    profile: AgentProfile::Codex,
                    activate: true,
                },
            )
            .expect("创建原卡")
        })
    });
    let CardCommandResult::AgentCreated {
        node_id: Some(node),
        ..
    } = created
    else {
        panic!("应有原生卡");
    };
    let hook = |name: &str, turn: &str| {
        parse_hook_event(
            HookSource::Codex,
            node,
            json!({"hook_event_name":name,"session_id":"input-control-session","turn_id":turn}),
        )
        .expect("Hook")
    };
    app.update(cx, |app, _| {
        app.enqueue_agent_hook_event_for_e2e(hook("SessionStart", "turn-a"))
            .expect("正常Ready路径")
    });
    crate::harness::drain_until(
        &app,
        cx,
        |runtime| {
            (runtime.agent_task_state(node) == Some(runtime::AgentTaskState::Ready)).then_some(())
        },
        Duration::from_millis(500),
    )
    .expect("原卡Ready");
    let (key, control_a) = app.update(cx, |app, _| {
        assert!(
            app.runtime
                .observe_mobile_hook(&hook("UserPromptSubmit", "turn-a")),
            "原回合"
        );
        assert!(
            app.runtime.observe_mobile_hook(&hook("Stop", "turn-a")),
            "原回合已结束"
        );
        let key = app
            .runtime
            .mobile_latest_turn_key(node)
            .expect("原回合身份");
        let control = runtime::TerminalInputLease::new(key.clone()).expect("Mac原任务租约");
        app.runtime
            .claim_mobile_terminal_input(&key, &control)
            .expect("接管");
        (key, control)
    });
    let dispatched = cx.update(|window, cx| {
        app.update(cx, |app, _| {
            app.execute_agent_card_command_for_e2e(
                window,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "ControlProbe".into(),
                        task: "queued-task".into(),
                    }],
                    force_untracked: false,
                },
            )
            .expect("应进入原派工队列")
        })
    });
    let CardCommandResult::AgentTasksDispatched { receipts } = dispatched else {
        panic!("派工回执");
    };
    assert_eq!(receipts.len(), 1, "一个派工只产生一份回执");
    assert_eq!(
        receipts[0].status,
        AgentTaskReceiptStatus::Queued,
        "接管时保留队列，不能误报Failed/Submitted"
    );
    // 500ms推进超过原100ms提交延迟，证明不是尚未来得及写入。
    let sent = crate::harness::drain_until(
        &app,
        cx,
        |_| (!writes.lock().expect("字节").is_empty()).then_some(()),
        Duration::from_millis(500),
    );
    assert!(sent.is_none(), "接管期间前台持续运行也不能自动写入");
    app.update(cx, |app, _| {
        app.runtime
            .release_mobile_terminal_input(&key, &control_a)
            .expect("交还原输入权")
    });
    crate::harness::drain_until(
        &app,
        cx,
        |_| (writes.lock().expect("字节").len() == 2).then_some(()),
        Duration::from_secs(1),
    )
    .expect("交还后原任务自动继续");
    assert_eq!(
        *writes.lock().expect("字节"),
        vec![b"queued-task".to_vec(), vec![0x0d]],
        "原派工正文与Enter各一次"
    );
    app.update(cx, |app, _| {
        app.enqueue_agent_hook_event_for_e2e(hook("UserPromptSubmit", "turn-task"))
            .expect("Provider接收");
        app.enqueue_agent_hook_event_for_e2e(hook("Stop", "turn-task"))
            .expect("Provider完成");
    });
    crate::harness::drain_until(
        &app,
        cx,
        |runtime| {
            (runtime.agent_task_state(node) == Some(runtime::AgentTaskState::Done)).then_some(())
        },
        Duration::from_millis(500),
    )
    .expect("原任务结束");
    let control_b = app.update(cx, |app, _| {
        let next = app
            .runtime
            .mobile_latest_turn_key(node)
            .expect("已完成的实际回合");
        let control = runtime::TerminalInputLease::new(next.clone()).expect("后续回合租约");
        app.runtime
            .claim_mobile_terminal_input(&next, &control)
            .expect("再次接管");
        control
    });
    let reply = cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.execute_norma_adapter_command_for_e2e(
                window,
                cx,
                agent::AdapterCommand::ReplyAgent {
                    agent: "ControlProbe".into(),
                    message: "queued-reply".into(),
                },
            )
            .expect("追问保留在原队列")
        })
    });
    assert_eq!(
        reply.reply_status.as_deref(),
        Some("queued"),
        "接管中的追问不得失败或谎报已发"
    );
    let sent = crate::harness::drain_until(
        &app,
        cx,
        |_| (writes.lock().expect("字节").len() > 2).then_some(()),
        Duration::from_millis(500),
    );
    assert!(sent.is_none(), "自动追问等待交还，不提前输入");
    // 模拟网络后台撤权：无前台交还调用，原队列仍须自动继续。
    std::thread::spawn(move || control_b.revoke())
        .join()
        .expect("后台撤权");
    crate::harness::drain_until(
        &app,
        cx,
        |_| (writes.lock().expect("字节").len() == 4).then_some(()),
        Duration::from_secs(1),
    )
    .expect("原追问继续输入");
    assert_eq!(
        *writes.lock().expect("字节"),
        vec![
            b"queued-task".to_vec(),
            vec![0x0d],
            b"queued-reply".to_vec(),
            vec![0x0d]
        ],
        "任务与追问各一次，不能重复或混成同一输入"
    );
    assert_eq!(
        app.read_with(cx, |app, _| app.runtime.live_len()),
        1,
        "全程复用唯一原PTY"
    );
}

/// 工作线程终端必须在同一次前台更新内核对原运行回合，不能跟随复用节点。
///
/// 数据构造：隔离项目、一个80×24录制PTY；同session中的turn-a与turn-b；
///   4秒请求预算=既有5秒手机预算减1秒传输余量，5秒测试预算含前台调度。
/// 执行过程：原Hook确认a → 读取原页 → 完成a后仍读原页 → 同节点提交b → 旧请求/旧route/过期请求。
/// 预期结果：原页及完成后的原页可读；下一回合、旧路由与超时均不返回正文；全程只有一个PTY。
/// 层级：headless GPUI + 录制PTY，不是实际App或真实模型验收。
#[test]
fn thread_terminal_reads_only_the_bound_provider_turn() {
    let mut test = TestAppContext::single();
    let (app, cx) = make_app(&mut test, crate::echo_factory());
    let workspace = tempfile::tempdir().expect("隔离项目");
    let _socket = enter_mobile_canvas(&app, cx, workspace.path());
    let created = cx.update(|window, cx| {
        app.update(cx, |app, _| {
            app.execute_agent_card_command_for_e2e(
                window,
                CardCommand::CreateAgent {
                    name: Some("TerminalProbe".into()),
                    profile: AgentProfile::Codex,
                    activate: true,
                },
            )
            .expect("创建已注册Agent")
        })
    });
    let CardCommandResult::AgentCreated {
        definition_id,
        node_id: Some(node),
        ..
    } = created
    else {
        panic!("必须激活原终端卡");
    };
    let hook = |name: &str, turn: &str| {
        parse_hook_event(
            HookSource::Codex,
            node,
            json!({"hook_event_name":name,"session_id":"terminal-session","turn_id":turn}),
        )
        .expect("明确测试Hook")
    };
    app.update(cx, |app, _| {
        assert!(
            app.runtime
                .observe_mobile_hook(&hook("UserPromptSubmit", "turn-a")),
            "原回合确认"
        );
    });
    let (workspace_id, route_id) = app.read_with(cx, |app, _| {
        app.norma_workspace_identity_for_e2e().expect("路由")
    });
    let key = app.read_with(cx, |app, _| {
        app.runtime.mobile_latest_turn_key(node).expect("准确回合")
    });
    let source = app.read_with(cx, |app, cx| {
        app.mobile_terminal_source_for_e2e(cx)
            .expect("正式终端通道")
    });
    let request = TerminalRequest {
        workspace_id,
        route_id,
        node_id: node,
        run_id: key.run_id.clone(),
        session_id: Some(key.session_id.clone()),
        before: None,
        revision: None,
    };
    let bound = LiveTerminalTask {
        task_ref: "test-runtime:7".into(),
        agent_definition_id: definition_id,
        key: key.clone(),
    };
    let read = |cx: &mut gpui::VisualTestContext,
                request: TerminalRequest,
                bound: LiveTerminalTask,
                deadline: Instant| {
        let (reply, response) = crossbeam_channel::bounded(1);
        source
            .try_send(LiveTerminalRequest {
                request,
                task: Some(bound.clone()),
                control: None,
                deadline,
                reply,
            })
            .expect("排入读取");
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            cx.executor().advance_clock(Duration::from_millis(20));
            cx.run_until_parked();
            if let Ok(result) = response.try_recv() {
                break result;
            }
            assert!(Instant::now() < until, "终端前台读取必须在5秒内结束");
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    let first = read(
        cx,
        request.clone(),
        bound.clone(),
        Instant::now() + Duration::from_secs(4),
    )
    .expect("原页");
    assert_eq!(first.card.node_id, node, "必须来自原PTY");
    assert_eq!(
        first.card.turn_id.as_deref(),
        Some("turn-a"),
        "必须来自原回合"
    );
    app.update(cx, |app, _| {
        assert!(
            app.runtime.observe_mobile_hook(&hook("Stop", "turn-a")),
            "完成原回合"
        );
    });
    assert!(
        read(
            cx,
            request.clone(),
            bound.clone(),
            Instant::now() + Duration::from_secs(4)
        )
        .is_ok(),
        "已完成的原回合仍可查看尚未复用的终端"
    );
    app.update(cx, |app, _| {
        assert!(
            app.runtime
                .observe_mobile_hook(&hook("UserPromptSubmit", "turn-b")),
            "开始下一回合"
        );
    });
    assert_eq!(
        read(
            cx,
            request.clone(),
            bound.clone(),
            Instant::now() + Duration::from_secs(4)
        )
        .err(),
        Some("session_changed"),
        "同run/session的下一回合不能冒充旧任务终端"
    );
    let mut old_route = request.clone();
    old_route.route_id = "expired-route".into();
    assert_eq!(
        read(
            cx,
            old_route,
            bound.clone(),
            Instant::now() + Duration::from_secs(4)
        )
        .err(),
        Some("route_expired"),
        "旧路由不能转到当前项目"
    );
    assert_eq!(
        read(cx, request.clone(), bound.clone(), Instant::now()).err(),
        Some("unavailable"),
        "过期请求不得读取或泄漏正文"
    );
    let mut current = bound.clone();
    current.key = app.read_with(cx, |app, _| {
        app.runtime.mobile_latest_turn_key(node).expect("下一回合")
    });
    assert!(
        read(
            cx,
            request.clone(),
            current.clone(),
            Instant::now() + Duration::from_secs(4)
        )
        .is_ok(),
        "匹配下一回合的独立授权读取应通过，不能一律禁用终端"
    );
    app.update(cx, |app, _| {
        assert!(
            app.register_external_agent_root_for_e2e(node, Some("新协调任务".into())),
            "建立新协调任务fixture"
        );
    });
    assert_eq!(
        read(
            cx,
            request,
            current,
            Instant::now() + Duration::from_secs(4)
        )
        .err(),
        Some("session_changed"),
        "即使新Provider Hook尚未到达，另一个协调任务也使旧task_ref不可读"
    );
    app.update(cx, |app, _| {
        assert_eq!(app.runtime.live_len(), 1, "不能为手机新开PTY");
        app.runtime.kill_all();
        assert!(
            app.runtime.mobile_latest_turn_key(node).is_none(),
            "清理后原终端必须不可用"
        );
    });
}
