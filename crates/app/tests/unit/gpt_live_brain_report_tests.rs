use super::session::CallCommand;
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
    let (reports, report_input) = async_channel::bounded(8);
    entity.update(cx, |app, cx| {
        let (generation, _) = app.gpt_live.begin();
        app.gpt_live.context_scope = Some(service.scope().clone());
        app.gpt_live.commands = Some(commands);
        app.gpt_live.reports = Some(reports);
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
    assert!(
        report_input.try_recv().is_ok(),
        "应实际向独立报告通道提交报告"
    );
    assert!(controls.is_empty(), "报告不应占用控制通道");
    runtime.block_on(delivery.flush()).unwrap();
    let snapshot = runtime.block_on(service.snapshot()).unwrap();
    assert_eq!(
        snapshot.state.reports[&report.reference.event_id].delivery,
        ReportDelivery::Skipped,
        "未处理命令释放后不能冒充发送或播放"
    );
}

/// 场景：幕僚报告入口积压时，静音、停播和松键必须仍可提交。
/// 构造：生产 App 实体和隔离报告账本，连接但暂不消费的容量为 8 的上游队列。
/// 执行：连续投递 9 条报告，再走生产静音、停播和松键入口。
/// 预期：前 8 条接收，第 9 条拒绝；控制均入队且松键不终止通话。
#[test]
fn gpt_live_report_backlog_preserves_call_controls() {
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
    let service = entity.update(cx, |app, _| app.shared_context_service().unwrap());
    let runtime = crate::app::shared_tokio_runtime().unwrap();
    let (commands, controls) = async_channel::bounded(8);
    let (reports, report_input) = async_channel::bounded(8);
    let cancellation = entity.update(cx, |app, _| {
        let (generation, cancellation) = app.gpt_live.begin();
        app.gpt_live.context_scope = Some(service.scope().clone());
        app.gpt_live.commands = Some(commands);
        app.gpt_live.reports = Some(reports);
        app.gpt_live.configure_input(true, true, true);
        app.gpt_live.apply(generation, CallEvent::Connected);
        cancellation
    });
    for index in 0..9 {
        let report = runtime
            .block_on(service.run(None, move |access| {
                access.report(&format!("backlog-{index}"), "范围需要确认", vec![])
            }))
            .unwrap();
        let delivery = service.report_delivery(report.reference).unwrap();
        let accepted = entity.update(cx, |app, _| {
            app.queue_live_brain_report(
                "范围需要确认".into(),
                norma::protocol::ReportPriority::Normal,
                delivery,
            )
        });
        assert_eq!(accepted, index < 8, "报告入口必须有界接收");
    }
    entity.update(cx, |app, cx| {
        assert!(app.gpt_live.request_mute().is_ok(), "报告不能挤掉静音控制");
        assert!(
            app.gpt_live.request_output().is_ok(),
            "报告不能挤掉停播控制"
        );
        app.release_gpt_live_input(cx);
        assert!(app.gpt_live.is_active(), "报告拥塞不能导致松键终止通话");
    });
    assert_eq!(report_input.len(), 8, "报告积压保持在独立入口");
    assert!(!cancellation.is_cancelled(), "通话取消令牌应保持有效");
    assert!(
        matches!(controls.try_recv(), Ok(CallCommand::SetMuted(true))),
        "静音命令必须真实入队"
    );
    assert!(
        matches!(controls.try_recv(), Ok(CallCommand::SetOutputBlocked(true))),
        "停播命令必须真实入队"
    );
    assert!(
        matches!(
            controls.try_recv(),
            Ok(CallCommand::SetInputEnabled { enabled: false, .. })
        ),
        "松键命令必须真实入队"
    );
}
