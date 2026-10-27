use super::*;

fn authorized_context(
    project: &str,
) -> (
    Arc<Mutex<live_voice::LiveContext>>,
    live_voice::DelegationSnapshot,
    agent::ToolContext,
) {
    let mut timeline = live_voice::LiveContext::default();
    timeline
        .begin_user_utterance(0.0)
        .expect("宿主确认的服务端边界");
    timeline
        .observe(live_voice::TranscriptFragment {
            event_id: "user-request".into(),
            speaker: live_voice::Speaker::User,
            delta: "创建一个浏览器".into(),
            start_ms: 10.0,
            end_ms: 200.0,
        })
        .expect("真实用户转写");
    let live_voice::DelegationRequest::Ready(snapshot) =
        timeline.prepare(&live_voice::DelegationNotice {
            id: "delegation".into(),
            offset_ms: 250.0,
        })
    else {
        panic!("完整授权片段应就绪")
    };
    assert!(timeline.claim(&snapshot), "此意图应首次认领");
    let context = agent::ToolContext {
        project_id: project.into(),
        turn_id: "host-intent:gpt-live:r2".into(),
        user_input: snapshot.user_input.clone(),
        input_source: agent::AgentInputSource::GptLiveVoice,
        contextual_delete_targets: Vec::new(),
        tool_call_id: "tool-1".into(),
        operation_id: Some("host-intent:gpt-live:r2:step-1".into()),
        cancellation: Default::default(),
    };
    (Arc::new(Mutex::new(timeline)), snapshot, context)
}

/// AC06/AC08/AC11：生产GPT Adapter创建真实画布节点；排队后取消或修订必须零新增。
/// 数据构造：隔离项目、echo PTY、宿主确认的Live快照与真实ToolContext。
/// 执行过程：合法请求创建Browser→取消token重放→新用户片段使revision失效后再调用。
/// 预期结果：首回执node存在；后两次被拒绝，节点始终只有1，不伪装豆包来源。
#[test]
fn gpt_live_adapter_rechecks_cancellation_and_revision_before_real_canvas_effect() {
    let mut gpui = gpui::TestAppContext::single();
    let workspace = tempfile::tempdir().expect("隔离项目");
    let (app, cx) = gpui.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    let project = workspace.path().display().to_string();
    let (timeline, snapshot, context) = authorized_context(&project);
    let generation = app.update(cx, |app, _| app.gpt_live.begin().0);
    let scope = LiveAdapterScope {
        project,
        generation,
        timeline: timeline.clone(),
        snapshot,
    };
    let run = |context: &agent::ToolContext, cx: &mut gpui::VisualTestContext| {
        cx.update(|window, app_cx| {
            app.update(app_cx, |app, cx| {
                execute_live_command(
                    app,
                    window,
                    cx,
                    &scope,
                    context,
                    AdapterCommand::CreateBrowser { url: None },
                )
            })
        })
    };
    let created = run(&context, cx).expect("授权请求应执行真实Adapter");
    assert_eq!(
        created.status, "succeeded",
        "创建画布卡片是已完成的同步副作用"
    );
    assert!(created.node_id.is_some(), "真实回执必须有node ID");
    context.cancellation.cancel();
    assert!(run(&context, cx).is_err(), "排队后取消必须在副作用前拒绝");
    let mut newer = context.clone();
    newer.cancellation = Default::default();
    timeline.lock().expect("时间线锁").suspend_user_input();
    assert!(run(&newer, cx).is_err(), "新语音冻结旧快照后不能执行旧请求");
    let count = app.read_with(cx, |app, _| app.runtime.document.nodes().len());
    assert_eq!(count, 1, "合法请求一张卡；取消与修订均不能新增第二张");
}

/// 场景：个人／总览（无项目）Live 通话追问共享状态，模型却试图建卡。
/// 构造：未打开项目的 App、空项目 scope 的 Live 授权快照与真实 ToolContext。
/// 执行：先走只读 ContextCheckpoint，再请求 CreateBrowser，最后冻结旧请求后重试 checkpoint。
/// 预期：checkpoint 成功；建卡被“只能核验共享状态”拒绝且无新增节点；旧请求冻结后连 checkpoint 也拒绝。
#[test]
fn gpt_live_personal_scope_verifies_context_but_rejects_project_actions() {
    let mut gpui = gpui::TestAppContext::single();
    let (app, cx) = gpui.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    let (timeline, snapshot, context) = authorized_context("");
    let generation = app.update(cx, |app, _| app.gpt_live.begin().0);
    let scope = LiveAdapterScope {
        project: String::new(),
        generation,
        timeline: timeline.clone(),
        snapshot,
    };
    let checkpoint = AdapterCommand::ContextCheckpoint {
        scope: app.update(cx, |app, _| {
            app.shared_context_service()
                .expect("个人共享作用域")
                .scope()
                .clone()
        }),
    };
    let run = |command: AdapterCommand, cx: &mut gpui::VisualTestContext| {
        cx.update(|window, app_cx| {
            app.update(app_cx, |app, cx| {
                execute_live_command(app, window, cx, &scope, &context, command)
            })
        })
    };
    let ok = run(checkpoint.clone(), cx).expect("个人只读核验应可达");
    assert_eq!(ok.status, "succeeded", "checkpoint 是已完成的只读核验");
    let denied = run(AdapterCommand::CreateBrowser { url: None }, cx).unwrap_err();
    assert!(
        denied.to_string().contains("只能核验共享状态"),
        "个人通话的项目动作必须拒绝：{denied}"
    );
    let nodes = app.read_with(cx, |app, _| app.runtime.document.nodes().len());
    assert_eq!(nodes, 0, "拒绝不能新增卡片");
    timeline.lock().expect("时间线锁").suspend_user_input();
    assert!(
        run(checkpoint, cx).is_err(),
        "新语音冻结旧快照后，旧核验也不能继续"
    );
}
