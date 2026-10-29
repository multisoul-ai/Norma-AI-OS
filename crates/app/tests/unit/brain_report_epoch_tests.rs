use super::*;
use agent::context::ReportDelivery;
use gpui::TestAppContext;
use norma::protocol::{BrainOp, CardCommand, ReportPriority, WireRequest};
use std::sync::{atomic::AtomicBool, atomic::Ordering, Arc, Mutex};

struct Fixture {
    entity: gpui::Entity<MultisoulApp>,
    cx: gpui::VisualTestContext,
    service: crate::app::shared_context::SharedContext,
    ids: (Option<String>, Option<String>),
    delivered: async_channel::Receiver<crate::app::doubao::DoubaoReport>,
    _dir: tempfile::TempDir,
    _test: TestAppContext,
}

fn set_speaking(f: &mut Fixture, speak: bool) {
    f.entity.update(&mut f.cx, |app, cx| {
        let mut settings = app.brain.settings.clone().unwrap();
        settings.speak = speak;
        app.apply_brain_settings(settings, false, cx);
    });
}

fn fixture(speak: bool) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let store = storage::AppStore::initialize(&paths).unwrap();
    let mut test = TestAppContext::single();
    let (entity, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    let (report_sender, delivered) = async_channel::bounded(4);
    let (service, ids) = entity.update(cx, |app, cx| {
        crate::norma::endpoint::install_at_path(dir.path().join("norma.sock"), cx).unwrap();
        app.enter_canvas_for_test(dir.path(), cx);
        app.register_loaded_project_route(None, cx);
        let (_, status) = std::sync::mpsc::channel();
        app.brain.host = Some(super::super::BrainHost {
            service: None,
            status,
            token: "report-token".into(),
        });
        let mut settings = config::BrainSettings::default();
        settings.enabled = true;
        settings.speak = speak;
        use crate::settings::voice::{
            DesktopVoiceMode, EnvVoiceSettings, SavedVoiceSettings, VoiceSettingsState,
        };
        app.voice_settings = VoiceSettingsState::from_sources(
            SavedVoiceSettings {
                desktop_mode: Some(DesktopVoiceMode::Doubao),
                doubao_credential_ref: Some("not-a-real-account".into()),
                ..Default::default()
            },
            EnvVoiceSettings::default(),
        );
        app.brain.settings = Some(settings);
        app.doubao.reports = Some(report_sender);
        (
            app.shared_context_service().unwrap(),
            (app.workspace_id.clone(), app.workspace_route_id.clone()),
        )
    });
    // Resolve the one-time scope recovery before the barrier starts holding storage.
    crate::app::shared_tokio_runtime()
        .unwrap()
        .block_on(service.snapshot())
        .unwrap();
    Fixture {
        entity,
        cx: cx.clone(),
        service,
        ids,
        delivered,
        _dir: dir,
        _test: test,
    }
}

/// Holds the serialized storage lane so a later report save queues behind it.
fn block_storage(f: &Fixture) -> std::sync::mpsc::Sender<()> {
    let (release, hold) = std::sync::mpsc::channel::<()>();
    let hold = Arc::new(Mutex::new(hold));
    let started = Arc::new(AtomicBool::new(false));
    let (service, flag) = (f.service.clone(), started.clone());
    crate::app::shared_tokio_runtime()
        .unwrap()
        .spawn(async move {
            let _ = service
                .run(None, move |_| {
                    flag.store(true, Ordering::Release);
                    let _ = hold.lock().unwrap().recv();
                    Ok(())
                })
                .await;
        });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while !started.load(Ordering::Acquire) {
        assert!(
            std::time::Instant::now() < deadline,
            "存储屏障必须先占住队列"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    release
}

fn only_report_status(f: &mut Fixture) -> ReportDelivery {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        f.cx.run_until_parked();
        let snapshot = crate::app::shared_tokio_runtime()
            .unwrap()
            .block_on(f.service.snapshot())
            .unwrap();
        if let Some(report) = snapshot.state.reports.values().next() {
            if report.delivery != ReportDelivery::Created {
                return report.delivery;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "报告必须保存并有界地给出交付结果"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

fn send_report(f: &mut Fixture) {
    let (response, _received) = crossbeam_channel::bounded(1);
    let request = WireRequest {
        protocol_version: norma::protocol::CURRENT_PROTOCOL_VERSION,
        client_version: None,
        request_id: 72,
        operation_id: None,
        authorization: None,
        source_workspace_id: f.ids.0.clone(),
        source_workspace_route_id: f.ids.1.clone(),
        source_node_id: None,
        target_workspace_id: None,
        command: CardCommand::Brain {
            token: "report-token".into(),
            op: BrainOp::Report {
                text: "部署已完成".into(),
                priority: ReportPriority::Normal,
                sources: vec![],
            },
        },
    };
    f.cx.update(|_, cx| crate::norma::endpoint::dispatch_test_request(request, response, cx));
    std::mem::forget(_received); // 响应不是本测试断言对象，避免发送端提前断开。
}

fn internal(f: &mut Fixture) {
    f.entity.update(&mut f.cx, |app, cx| {
        app.speak_brain_report("部署已完成".into(), ReportPriority::Normal, cx)
    });
}

/// 场景：内部提醒与 CLI 报告都在后台保存期间，用户关闭再开启幕僚播报。
/// 构造：播报开启；用存储屏障占住共享上下文队列，使保存晚于关闭→开启。
/// 执行：分别从 speak_brain_report 与 Report 端点发出报告，关闭、开启后释放屏障。
/// 预期：正文仍保存但状态 Skipped，音频通道没有收到迟到报告。
#[test]
fn brain_report_closed_then_reopened_during_save_is_skipped_for_both_entries() {
    for entry in [internal as fn(&mut Fixture), send_report] {
        let mut f = fixture(true);
        let release = block_storage(&f);
        entry(&mut f);
        set_speaking(&mut f, false);
        set_speaking(&mut f, true);
        release.send(()).unwrap();
        assert_eq!(
            only_report_status(&mut f),
            ReportDelivery::Skipped,
            "保存中关闭再开启，旧报告不能进入新开启的通道"
        );
        assert!(f.delivered.try_recv().is_err(), "迟到报告不得补播");
    }
}

/// 场景：报告在播报关闭期间到达，保存未完成时用户重新开启。
/// 构造：播报关闭；存储屏障占住队列。
/// 执行：两个入口分别发报告，开启播报后释放屏障。
/// 预期：到达时无交付资格，开启不能补播：状态 Skipped，音频通道为空。
#[test]
fn brain_report_received_while_muted_is_not_replayed_after_reopen() {
    for entry in [internal as fn(&mut Fixture), send_report] {
        let mut f = fixture(false);
        let release = block_storage(&f);
        entry(&mut f);
        set_speaking(&mut f, true);
        release.send(()).unwrap();
        assert_eq!(only_report_status(&mut f), ReportDelivery::Skipped);
        assert!(
            f.delivered.try_recv().is_err(),
            "关闭期间收到的报告不得补播"
        );
    }
}

/// 场景：保存期间没有任何停播或切换，报告应正常交付。
/// 构造：播报开启，存储屏障短暂延迟保存。
/// 执行：两个入口发报告并释放屏障。
/// 预期：状态为 Queued（已进入播报通道），不被新门禁误杀。
#[test]
fn brain_report_without_cancellation_is_still_delivered_after_slow_save() {
    for entry in [internal as fn(&mut Fixture), send_report] {
        let mut f = fixture(true);
        let release = block_storage(&f);
        entry(&mut f);
        release.send(()).unwrap();
        assert_eq!(only_report_status(&mut f), ReportDelivery::Queued);
    }
}
