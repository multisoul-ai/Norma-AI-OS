use super::*;
use agent::context::{ContextOrigin, ContextPayload, ReportDelivery};
use gpui::TestAppContext;
use norma::protocol::{BrainOp, CardCommand, CardCommandResult, ReportPriority, WireRequest};

/// 场景：宿主主动提醒用户需要确认，而语音汇报关闭。
/// 构造：隔离数据库和个人画布，宿主报告没有可证实的外部来源引用。
/// 执行：调用生产 speak_brain_report，推进后台保存后读取共享快照。
/// 预期：正文可供后续追问，状态为 Skipped，不能因为不播音而丢失报告。
#[test]
fn brain_report_context_internal_reminder_is_saved_without_speech() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let store = storage::AppStore::initialize(&paths).unwrap();
    let mut test = TestAppContext::single();
    let (entity, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    let service = entity.update(cx, |app, cx| {
        let service = app.shared_context_service().unwrap();
        app.speak_brain_report("需要你确认：部署尚未批准".into(), ReportPriority::Now, cx);
        service
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        cx.run_until_parked();
        let snapshot = crate::app::shared_tokio_runtime()
            .unwrap()
            .block_on(service.snapshot())
            .unwrap();
        if let Some(report) = snapshot.state.reports.values().next() {
            if report.delivery == ReportDelivery::Skipped {
                assert_eq!(
                    report.text, "需要你确认：部署尚未批准",
                    "未播出的报告仍须可读"
                );
                let reference = report.reference.clone();
                let record = crate::app::shared_tokio_runtime()
                    .unwrap()
                    .block_on(service.run(None, move |access| access.read(&reference)))
                    .unwrap();
                assert!(record.event.related.is_empty(), "缺失的历史来源不得猜造");
                return;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "内部提醒也必须先保存再交付，关闭语音时应保存为 Skipped"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// 场景：幕僚带来源的报告即使关闭播报，也应可被随后追问，并明确记录未播放。
/// 构造：隔离数据库、真实 endpoint 路由、有效幕僚令牌和已保存的用户输入引用。
/// 执行：从生产请求分发入口提交 Report，等待后台保存及交付决定，再读取共享快照。
/// 预期：返回 saved=true、稳定引用和 queued=false；报告正文及来源存在，状态为 Skipped。
#[test]
fn brain_report_context_persists_sources_before_disabled_delivery() {
    report_fixture(false, None);
}

/// 场景：已保存并排队的幕僚报告在开播前取消。
/// 构造：豆包仅配置虚拟账户但不连接通道，报告留在等待队列。
/// 执行：真实 endpoint 入队后调用生产取消入口，再读取共享状态。
/// 预期：Queued 变为 Skipped，不能永久占用活动报告名额，也不能误报已播放。
#[test]
fn brain_report_context_cancelled_queue_records_skipped() {
    report_fixture(true, None);
}

/// 场景：报告来源越界、版本错误、来源不存在或持久化失败。
/// 构造：有效幕僚身份和可交付的豆包记录通道；分别篡改引用及用 SQLite trigger 拒绝保存。
/// 执行：从正式端点提交报告，检查错误、共享快照及播报通道。
/// 预期：精确错误有界返回，既不保存半条报告，也不向音频通道发送正文。
#[test]
fn brain_report_context_rejects_bad_sources_and_failed_save_before_delivery() {
    for fault in [
        "scope_mismatch",
        "stale_snapshot",
        "unavailable",
        "test_report_write_failure",
    ] {
        report_fixture(true, Some(fault));
    }
}

fn report_fixture(speak: bool, fault: Option<&str>) {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let database = paths.app_db_path().to_path_buf();
    let store = storage::AppStore::initialize(&paths).unwrap();
    let mut test = TestAppContext::single();
    let (entity, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    let (service, workspace, route) = entity.update(cx, |app, cx| {
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
        if speak {
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
        }
        app.brain.settings = Some(settings);
        (
            app.shared_context_service().unwrap(),
            app.workspace_id.clone(),
            app.workspace_route_id.clone(),
        )
    });
    let mut source = crate::app::shared_tokio_runtime()
        .unwrap()
        .block_on(service.run(None, |access| {
            access.record_input("question", ContextOrigin::Voice, "刚才为什么拦下")
        }))
        .unwrap()
        .reference;
    let (report_sender, delivered) = async_channel::bounded(4);
    if let Some(fault) = fault {
        entity.update(cx, |app, _| app.doubao.reports = Some(report_sender));
        match fault {
            "scope_mismatch" => source.scope.area_id = "different-area".into(),
            "stale_snapshot" => source.revision += 1,
            "unavailable" => source.event_id = "missing-source".into(),
            "test_report_write_failure" => {
                rusqlite::Connection::open(&database).unwrap().execute_batch(
                    "CREATE TRIGGER fail_report BEFORE INSERT ON shared_context_events BEGIN SELECT RAISE(ABORT, 'test_report_write_failure'); END;"
                ).unwrap();
            }
            _ => unreachable!(),
        }
    }
    let (response, received) = crossbeam_channel::bounded(1);
    let request = WireRequest {
        protocol_version: norma::protocol::CURRENT_PROTOCOL_VERSION,
        client_version: None,
        request_id: 71,
        operation_id: None,
        authorization: None,
        source_workspace_id: workspace,
        source_workspace_route_id: route,
        source_node_id: None,
        target_workspace_id: None,
        command: CardCommand::Brain {
            token: "report-token".into(),
            op: BrainOp::Report {
                text: "因为当前任务尚未获得批准".into(),
                priority: ReportPriority::Normal,
                sources: vec![source.clone()],
            },
        },
    };
    cx.update(|_, cx| crate::norma::endpoint::dispatch_test_request(request, response, cx));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let response = loop {
        cx.run_until_parked();
        if let Ok(response) = received.try_recv() {
            break response;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "报告保存与明确跳过必须有界返回"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    if let Some(fault) = fault {
        let error = response.result.unwrap_err().to_string();
        assert!(
            error.contains(fault),
            "必须返回真实拒绝原因 {fault}，实际为 {error}"
        );
        let snapshot = crate::app::shared_tokio_runtime()
            .unwrap()
            .block_on(service.snapshot())
            .unwrap();
        assert!(snapshot.state.reports.is_empty(), "失败事务不能留下报告");
        assert!(
            delivered.try_recv().is_err(),
            "来源或保存失败不得投递音频通道"
        );
        assert!(
            entity.read_with(cx, |app, _| app.agent_announcement_audio_idle()),
            "失败后不得占用播报状态"
        );
        return;
    }
    let CardCommandResult::Brain { output } = response.result.expect("有效来源报告应接受")
    else {
        panic!("必须是幕僚回执");
    };
    let output: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(
        output["saved"], true,
        "成功回执必须以 durable commit 为前提"
    );
    assert_eq!(output["queued"], speak, "回执必须对应实际入队结果");
    let reference: agent::context::ContextRef =
        serde_json::from_value(output["reference"].clone()).unwrap();
    let (record, snapshot) = crate::app::shared_tokio_runtime()
        .unwrap()
        .block_on(service.run(None, move |access| {
            Ok((access.read(&reference)?, access.snapshot()?))
        }))
        .unwrap();
    assert_eq!(record.event.related, vec![source], "来源引用必须完整保留");
    assert_eq!(
        record.event.payload,
        ContextPayload::Report {
            text: "因为当前任务尚未获得批准".into()
        },
        "正文应先保存供追问"
    );
    assert_eq!(
        snapshot.state.reports[&record.reference.event_id].delivery,
        if speak {
            ReportDelivery::Queued
        } else {
            ReportDelivery::Skipped
        },
        "入队与关闭播报必须区分，不能写成 Completed"
    );
    if speak {
        entity.update(cx, |app, _| app.cancel_agent_announcements());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            cx.run_until_parked();
            let snapshot = crate::app::shared_tokio_runtime()
                .unwrap()
                .block_on(service.snapshot())
                .unwrap();
            let status = snapshot.state.reports[&record.reference.event_id].delivery;
            if status == ReportDelivery::Skipped {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "取消前未开播的报告必须落库为 Skipped，实际为 {status:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}
