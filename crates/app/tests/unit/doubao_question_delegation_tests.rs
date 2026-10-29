use super::*;
use crate::app::MultisoulApp;
use std::sync::atomic::{AtomicUsize, Ordering};
#[path = "../support/brain_question.rs"]
mod support;

/// 场景：豆包完整转写的明确代答直接进入当前原题，失效通话不创建授权。
/// 构造：隔离App/SQLite/Hook、生产context通道与ChatRuntime；本地代理阻断所有模型/TTS网络。
/// 执行：respond实际发送到GPUI宿主，覆盖正常、取消、generation、project和scope变化。
/// 预期：正常只授予一次当前题并返回委托回执；其余无授权，均不请求普通模型上下文或派任务。
#[test]
fn doubao_question_delegation_uses_host_scope_without_model_planning() {
    for scenario in ["active", "cancelled", "generation", "project", "scope"] {
        let dir = tempfile::tempdir().unwrap();
        let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
        let database = paths.app_db_path().to_path_buf();
        let runtime = crate::app::shared_tokio_runtime().unwrap();
        let mut test = gpui::TestAppContext::single();
        let (source, cx) = test.add_window_view(|_, cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                paths,
            )
        });
        let (target, context) = source.update(cx, |app, cx| {
            let raw = serde_json::from_str(include_str!(
                "../../../norma/tests/fixtures/questions/claude-pre-tool-use.json"
            ))
            .unwrap();
            let (target, _, _) = support::make_question_from_raw(app, cx, dir.path(), raw);
            app.runtime.focus.exit_to_canvas();
            app.runtime.selection.select_only(target.node_id);
            app.install_question_brain_for_test();
            app.doubao.cancellation = Some(CancellationToken::new());
            let context = app.start_doubao_context_requests(
                app.doubao.generation,
                Some(dir.path().display().to_string()),
                cx,
            );
            match scenario {
                "generation" => app.doubao.generation += 1,
                "project" => app.runtime.set_workspace_root(dir.path().join("elsewhere")),
                "scope" => app.shared_context_scope.as_mut().unwrap().area_id = "elsewhere".into(),
                _ => {}
            }
            (target, context)
        });
        let models = Arc::new(AtomicUsize::new(0));
        let counted = models.clone();
        let (sender, requests) = async_channel::bounded(1);
        let forwarding = runtime.spawn(async move {
            while let Ok(request) = requests.recv().await {
                if matches!(&request, ContextRequest::Model(_)) {
                    counted.fetch_add(1, Ordering::SeqCst);
                }
                if context.send(request).await.is_err() {
                    break;
                }
            }
        });
        let proxy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let (bus, _wake) = crate::app::voice_window_inset::VoiceWindowInsetEventBus::new();
        let playback = Arc::new(crate::app::doubao::playback::start_playback_with(
            || Box::new(crate::app::voice::playback::RecordingAssistantAudioSink::new()),
            bus.sink(crate::app::voice_window_inset::TtsPlaybackSource::Doubao),
        ));
        let chat = Arc::new(ChatRuntime {
            client: Client::builder()
                .proxy(
                    reqwest::Proxy::all(format!("http://{}", proxy.local_addr().unwrap())).unwrap(),
                )
                .timeout(std::time::Duration::from_millis(250))
                .build()
                .unwrap(),
            key: "fixture".into(),
            voice: ::tts::providers::doubao::default_voice(),
            context: sender,
            history: Default::default(),
            memory_epoch: Default::default(),
            memory_revisions: Default::default(),
            tools: None,
            project: Some(dir.path().display().to_string()),
            database,
            session: "question-test".into(),
            playback,
        });
        let cancellation = CancellationToken::new();
        if scenario == "cancelled" {
            cancellation.cancel();
        }
        let (status, updates) = async_channel::bounded(4);
        let work = runtime.spawn(chat.respond("这题你直接回答".into(), 1, cancellation, status));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
        while !work.is_finished() {
            cx.run_until_parked();
            assert!(
                std::time::Instant::now() < deadline,
                "{scenario} 本地分支有界结束"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let _ = runtime
            .block_on(work)
            .expect("真实回复任务不可 panic；仅允许预期 TTS/关闭通道错误");
        forwarding.abort();
        let messages: Vec<_> = std::iter::from_fn(|| updates.try_recv().ok()).collect();
        source.read_with(cx, |app, _| {
            assert_eq!(
                app.question_delegated_for_test(&target),
                scenario == "active",
                "{scenario} 宿主按原话及有效通话授权"
            );
            assert!(
                app.agent_task_coordinator
                    .as_ref()
                    .is_none_or(|c| c.tasks().is_empty()),
                "不能创建普通任务"
            );
        });
        assert_eq!(
            models.load(Ordering::SeqCst),
            0,
            "{scenario} 不回落普通模型规划"
        );
        if scenario == "active" {
            assert!(messages.iter().any(|event| matches!(event, DoubaoStatus::AssistantReply(text) if text.contains("已委托"))), "真实委托回执进入播报链路");
        }
    }
}
