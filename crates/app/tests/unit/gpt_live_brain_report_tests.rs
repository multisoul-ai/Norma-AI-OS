use super::*;
use crate::app::{brain_host::BrainSettings, shared_context::report::ReportDeliveryHandle};
use agent::context::ReportDelivery;
use gpui::TestAppContext;

/// 场景：已有 Live 通话时，幕僚报告应交给原生通道，不能因旧 TTS 不可用被跳过。
/// 构造：隔离 App 和报告账本；启用语音汇报，模拟已连接的 Live 控制通道。
/// 执行：调用生产 speak_saved_brain_report，随后释放尚未处理的队列命令。
/// 预期：报告进入原生队列，旧播报队列保持空；未发送而丢弃后记录 Skipped。
#[test]
fn gpt_live_brain_report_uses_native_queue_without_legacy_tts() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut test = TestAppContext::single();
    let (entity, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    let service = entity.update(cx, |app, cx| {
        app.apply_brain_settings(
            BrainSettings {
                enabled: true,
                speak: true,
                ..Default::default()
            },
            false,
            cx,
        );
        app.voice_settings
            .select_desktop_mode(DesktopVoiceMode::GptLive);
        app.shared_context_service().unwrap()
    });
    let runtime = crate::app::shared_tokio_runtime().unwrap();
    let report = runtime
        .block_on(service.run(None, |access| {
            access.report("native", "任务需要先确认范围", vec![])
        }))
        .unwrap();
    let delivery: ReportDeliveryHandle = service.report_delivery(report.reference.clone()).unwrap();
    let (commands, controls) = async_channel::bounded(8);
    entity.update(cx, |app, cx| {
        let (generation, _) = app.gpt_live.begin();
        app.gpt_live.context_scope = Some(service.scope().clone());
        app.gpt_live.commands = Some(commands);
        app.gpt_live.apply(generation, CallEvent::Connected);
        let queued = app.speak_saved_brain_report(
            "任务需要先确认范围".into(),
            norma::protocol::ReportPriority::Normal,
            delivery.clone(),
            cx,
        );
        assert!(queued, "已连接 Live 必须能接收幕僚报告");
        assert!(app.agent_announcement_audio_idle(), "旧播放器应保持空闲");
    });
    assert!(controls.try_recv().is_ok(), "应实际向原生控制通道提交报告");
    runtime.block_on(delivery.flush()).unwrap();
    let snapshot = runtime.block_on(service.snapshot()).unwrap();
    assert_eq!(
        snapshot.state.reports[&report.reference.event_id].delivery,
        ReportDelivery::Skipped,
        "未处理命令释放后不能冒充发送或播放"
    );
}
