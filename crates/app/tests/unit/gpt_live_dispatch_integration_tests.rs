use super::super::bridge::LiveBridge;
use super::*;
use agent::{AgentCardAdapter, AgentInputSource, ProviderConfig, ProviderProtocol, ToolContext};
use serde_json::json;
#[path = "../support/gpt_live_sse.rs"]
mod sse;

/// AC05/AC07/AC08/AC10：本地SSE规划经真实NormaSession、持久Adapter和GPUI pump创建隔离项目卡片。
/// 构造：观测原文“创建浏览器”、模型create_browser和最终回复；真实App域状态及隔离SQLite。
/// 执行：认领→SSE工具调用→生产channel/MemoryAdapter/画布副作用→真实回执；换owner重放相同操作。
/// 预期：真实Browser只有一张；node回执与账本一致；重放回执不新增卡片；不代表真人或云端验收。
#[test]
fn gpt_live_sse_to_real_project_adapter_records_one_browser_and_replays_receipt() {
    let runtime = crate::app::shared_tokio_runtime().expect("Tokio runtime");
    let (endpoint,server)=runtime.block_on(sse::serve(vec![
        json!({"role":"assistant","tool_calls":[{"index":0,"id":"tool-fixture","type":"function","function":{"name":"create_browser","arguments":"{}"}}]}),
        json!({"role":"assistant","content":"浏览器已创建"}),
    ]));
    let mut gpui = gpui::TestAppContext::single();
    let project = tempfile::tempdir().expect("隔离项目");
    let (app, cx) = gpui.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    app.update(cx, |app, cx| app.enter_canvas_for_test(project.path(), cx));
    let (generation, cancellation) = app.update(cx, |app, _| app.gpt_live.begin());
    let mut bridge = LiveBridge::new("fixture-call".into(), cancellation);
    bridge
        .transcript(live_voice::TranscriptFragment {
            event_id: "u1".into(),
            speaker: live_voice::Speaker::User,
            delta: "创建一个浏览器".into(),
            start_ms: 0.0,
            end_ms: 200.0,
        })
        .expect("真实观测原文");
    bridge
        .delegation(live_voice::DelegationNotice {
            id: "d1".into(),
            offset_ms: 150.0,
        })
        .expect("模型委派");
    let work = bridge.claim_next().expect("按观测原文规划");
    let project_id = project.path().display().to_string();
    let scope = LiveAdapterScope {
        project: project_id.clone(),
        generation,
        timeline: bridge.timeline.clone(),
        snapshot: work.snapshot.clone(),
    };
    let delegate = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            start_gpt_live_adapter(scope, app.database_paths.clone(), window, cx)
        })
    });
    let database = project.path().join("live-app.sqlite");
    let durable = Arc::new(LiveDurableAdapter::new(
        delegate.clone(),
        database.clone(),
        project_id.clone(),
        work.intent.clone(),
        "owner-a".into(),
    ));
    let (done, finished) = async_channel::bounded(1);
    let (adapter, intent, snapshot, token, session_project) = (
        durable.clone(),
        work.intent.clone(),
        work.snapshot.clone(),
        work.cancellation.clone(),
        project_id.clone(),
    );
    runtime.spawn(async move {
        let config = ProviderConfig::new(
            ProviderProtocol::OpenAiCompatible,
            endpoint,
            "fixture",
            Some("fake-key".into()),
        );
        let mut session =
            agent::NormaSession::new(session_project, &config, adapter).expect("真实后台会话");
        let result = session
            .turn_delegated(agent::NormaDelegation {
                request_id: intent,
                revision: snapshot.revision,
                user_input: snapshot.user_input,
                conversation: "User: 创建一个浏览器".into(),
                cancellation: token,
            })
            .await;
        let _ = done.send(result).await;
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let result = loop {
        cx.run_until_parked();
        if let Ok(result) = finished.try_recv() {
            break result.unwrap_or_else(|error| {
                panic!(
                    "SSE委派应完成：{error:?}; 实际回执={:?}; 原始落账={:?}",
                    durable.receipts(),
                    storage::LiveOperationStore::new(database.clone()).recent(&project_id, 5)
                )
            });
        }
        assert!(
            std::time::Instant::now() < deadline,
            "生产pump应在5秒内完成"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    assert_eq!(result.tool_calls, 1, "真实模型只发出一次工具调用");
    let (pending, receipts) = durable.completion_snapshot();
    let node = receipts[0]
        .as_ref()
        .expect("实际成功回执")
        .node_id
        .expect("真实node");
    let content = bridge
        .complete(
            &work,
            super::super::report::DelegatedReport {
                receipts,
                pending,
                effects: true,
                planner: Ok(Some(result.final_text)),
            },
        )
        .expect("回传真实结果");
    assert!(
        content.contains(&format!("Some({node})")),
        "Live报告包含真实画布身份"
    );
    let turn_id = format!("{}:gpt-live:r{}", work.intent, work.snapshot.revision);
    let context = ToolContext {
        project_id: project_id.clone(),
        turn_id: turn_id.clone(),
        user_input: work.snapshot.user_input.clone(),
        input_source: AgentInputSource::GptLiveVoice,
        contextual_delete_targets: vec![],
        tool_call_id: "replay".into(),
        operation_id: Some(format!("{turn_id}:step-1")),
        cancellation: Default::default(),
    };
    let restarted = LiveDurableAdapter::new(
        delegate,
        database,
        project_id,
        work.intent,
        "owner-b".into(),
    );
    let replay = runtime
        .block_on(restarted.execute(agent::AdapterCommand::CreateBrowser { url: None }, &context))
        .expect("重启重放");
    assert_eq!(replay.node_id, Some(node), "重启只返回原回执");
    let count = app.read_with(cx, |app, _| app.runtime.document.nodes().len());
    assert_eq!(count, 1, "真实项目只有一张Browser，重放不新增");
    let requests = runtime.block_on(server).expect("SSE服务结束");
    assert_eq!(requests.len(), 2, "工具调用和最终回复均完成真实HTTP交换");
    app.update(cx, |app, cx| app.stop_gpt_live_voice(cx));
}
