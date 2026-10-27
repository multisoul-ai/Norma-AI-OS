use super::*;
use agent::context::{ContextPayload, ContextRef, DecisionStatus, ReceiptStatus};
use gpui::TestAppContext;
use norma::protocol::{AgentProfile, BrainOp, CardCommand, CardCommandResult, WireRequest};

/// 场景：幕僚派发任务后，需要把真实派发回执与决定作为后续报告的来源。
/// 构造：隔离数据库、正式端点路由、已就绪的 Alan 卡片；PTY 只记录接收，不运行 Agent。
/// 执行：通过生产端点 Assign，再沿返回引用读取决定和回执，最后提交引用这两项的 Report。
/// 预期：引用可读、关联正确；回执仅为 Accepted，不能把提交任务说成任务完成。
#[test]
fn brain_action_context_assign_returns_reportable_sources() {
    assign_fixture("Alan".into(), false);
}

/// 场景：可路由的 Agent 地址带大量首尾空白，不能令成功派发的来源记录超长。
/// 构造：9000 个空格加 Alan，超过上下文正文 8192 字节，但地址解析仍定位同一卡片。
/// 执行：走完整 Assign 和 Report 端点链路。
/// 预期：保存规范目标名称，来源仍可读，不因请求写法丢失实际回执。
#[test]
fn brain_action_context_assign_uses_canonical_receipt_name() {
    assign_fixture(format!("{}Alan", " ".repeat(9000)), false);
}

/// 场景：任务已实际接收，但共享回执写入失败；调用者必须知道不能重新派发。
/// 构造：隔离库触发器只拒绝 Receipt，允许 Decision 提交；记录型 PTY 统计任务正文写入。
/// 执行：从端点派发，读取失败的保存状态，再恢复数据库并检查真实任务写入次数。
/// 预期：保持成功任务回执、saved=false、retry_action=false；不伪造来源，不自动重复派发。
#[test]
fn brain_action_context_save_failure_keeps_actual_acceptance() {
    assign_fixture("Alan".into(), true);
}

struct TaskPty(std::sync::Arc<std::sync::atomic::AtomicUsize>);
impl terminal::PtyBackend for TaskPty {
    fn write(&mut self, bytes: &[u8]) -> Result<(), terminal::PtyError> {
        let task = "检查边界条件".as_bytes();
        if bytes.windows(task.len()).any(|part| part == task) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(())
    }
    fn resize(&mut self, _: u16, _: u16) -> Result<(), terminal::PtyError> {
        Ok(())
    }
    fn kill(&mut self) {}
}

fn assign_fixture(address: String, fail_receipt: bool) {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let database = paths.app_db_path().to_path_buf();
    let writes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let captured = writes.clone();
    let mut test = TestAppContext::single();
    let (entity, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(move |_, _, _, _| Ok(Box::new(TaskPty(captured.clone())))),
            paths,
        )
    });
    let (service, workspace, route) = entity.update(cx, |app, cx| {
        crate::norma::endpoint::install_at_path(dir.path().join("norma.sock"), cx).unwrap();
        app.enter_canvas_for_test(dir.path(), cx);
        app.register_loaded_project_route(None, cx);
        let (_, status) = std::sync::mpsc::channel();
        app.brain.host = Some(super::super::BrainHost {
            service: None,
            status,
            token: "action-token".into(),
        });
        let mut settings = config::BrainSettings::default();
        settings.autonomy.assign_followups = true;
        app.brain.settings = Some(settings);
        (
            app.shared_context_service().unwrap(),
            app.workspace_id.clone(),
            app.workspace_route_id.clone(),
        )
    });
    let node = entity.update_in(cx, |app, window, _| {
        let created = crate::app::execute_agent_card_command(
            app,
            window,
            CardCommand::CreateAgent {
                name: Some("Alan".into()),
                profile: AgentProfile::Codex,
                activate: true,
            },
        )
        .unwrap();
        let CardCommandResult::AgentCreated {
            node_id: Some(node),
            ..
        } = created
        else {
            panic!("应返回创建的卡片");
        };
        app.pending_agent_submissions.mark_ready(node);
        node
    });
    let send = |op| WireRequest {
        protocol_version: norma::protocol::CURRENT_PROTOCOL_VERSION,
        client_version: None,
        request_id: 72,
        operation_id: None,
        authorization: None,
        source_workspace_id: workspace.clone(),
        source_workspace_route_id: route.clone(),
        source_node_id: None,
        target_workspace_id: None,
        command: CardCommand::Brain {
            token: "action-token".into(),
            op,
        },
    };
    crate::app::shared_tokio_runtime()
        .unwrap()
        .block_on(service.snapshot())
        .unwrap();
    if fail_receipt {
        rusqlite::Connection::open(&database).unwrap().execute_batch(
            "CREATE TRIGGER fail_action_receipt BEFORE INSERT ON shared_context_events WHEN json_extract(NEW.payload,'$.payload.kind')='receipt' BEGIN SELECT RAISE(ABORT,'test_action_receipt_failure'); END;"
        ).unwrap();
    }
    let output = send_request(
        cx,
        send(BrainOp::Assign {
            agent: address,
            task: "检查边界条件".into(),
        }),
    );
    assert_eq!(
        writes.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "必须实际提交一次任务，不能只检查预生成回执"
    );
    if fail_receipt {
        assert_eq!(
            output["action_accepted"], true,
            "保存失败不能改变真实接收结果"
        );
        assert_eq!(output["context"]["saved"], false, "不可声称来源已经提交");
        assert_eq!(
            output["context"]["retry_action"], false,
            "不能指导调用者重复派发"
        );
        assert!(
            output["context"]["error"]
                .as_str()
                .is_some_and(|e| e.contains("test_action_receipt_failure")),
            "应说明真实保存错误"
        );
        assert!(
            output["context"]["sources"].is_null(),
            "不能返回未提交的引用"
        );
        assert_eq!(
            output["receipt"]["target_node_id"], node,
            "仍应返回真正接收任务的目标"
        );
        rusqlite::Connection::open(database)
            .unwrap()
            .execute_batch("DROP TRIGGER fail_action_receipt;")
            .unwrap();
        let snapshot = crate::app::shared_tokio_runtime()
            .unwrap()
            .block_on(service.snapshot())
            .unwrap();
        assert_eq!(
            snapshot
                .events
                .iter()
                .filter(|r| matches!(r.event.payload, ContextPayload::Decision { .. }))
                .count(),
            1,
            "决策已提交的部分结果必须真实可见"
        );
        assert!(
            !snapshot
                .events
                .iter()
                .any(|r| matches!(r.event.payload, ContextPayload::Receipt { .. })),
            "不能伪造已保存回执"
        );
        assert_eq!(
            writes.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "恢复读取不能重做动作"
        );
        let tasks = entity.read_with(cx, |app, _| {
            app.agent_task_coordinator.as_ref().unwrap().tasks()
        });
        assert_eq!(tasks.len(), 1, "不能隐藏第二个尚未写入 PTY 的重复排队任务");
        assert_eq!(
            tasks[0].task_ref,
            output["receipt"]["task_id"].as_str().unwrap(),
            "唯一任务应与真实回执一致"
        );
        assert_ne!(tasks[0].state, "queued", "此任务已实际提交，不是仅排队");
        return;
    }
    assert_eq!(output["context"]["saved"], true, "回执应附带已提交的来源");
    assert_eq!(
        output["receipt"]["target_node_id"], node,
        "应保留实际任务回执"
    );
    let sources: Vec<ContextRef> =
        serde_json::from_value(output["context"]["sources"].clone()).unwrap();
    assert_eq!(sources.len(), 2, "应有一个决定和一个实际回执");
    let references = sources.clone();
    let records = crate::app::shared_tokio_runtime()
        .unwrap()
        .block_on(service.run(None, move |access| {
            references
                .iter()
                .map(|reference| access.read(reference))
                .collect::<storage::StorageResult<Vec<_>>>()
        }))
        .unwrap();
    assert!(
        matches!(
            records[0].event.payload,
            ContextPayload::Decision {
                status: DecisionStatus::Allowed,
                ..
            }
        ),
        "派发已通过宿主权限校验"
    );
    let ContextPayload::Receipt {
        status,
        operation_id,
        summary,
    } = &records[1].event.payload
    else {
        panic!("第二项必须为回执")
    };
    assert_eq!(*status, ReceiptStatus::Accepted, "派发接收不等于任务完成");
    assert!(
        summary.contains("检查边界条件") && summary.contains("Alan"),
        "回执应说明实际派发对象和任务"
    );
    assert_eq!(
        operation_id,
        output["receipt"]["task_id"].as_str().unwrap(),
        "回执身份必须来自真实任务"
    );
    assert_eq!(
        records[1].event.related,
        vec![sources[0].clone()],
        "回执必须关联对应决定"
    );
    let report = send_request(
        cx,
        send(BrainOp::Report {
            text: "已交给 Alan 检查边界条件，等待执行结果".into(),
            priority: norma::protocol::ReportPriority::Normal,
            sources: sources.clone(),
        }),
    );
    let reference: ContextRef = serde_json::from_value(report["reference"].clone()).unwrap();
    let record = crate::app::shared_tokio_runtime()
        .unwrap()
        .block_on(service.run(None, move |access| access.read(&reference)))
        .unwrap();
    assert_eq!(
        record.event.related, sources,
        "播报应沿用真实决定与回执引用"
    );
}

fn send_request(cx: &mut gpui::VisualTestContext, request: WireRequest) -> serde_json::Value {
    let (response, received) = crossbeam_channel::bounded(1);
    cx.update(|_, cx| crate::norma::endpoint::dispatch_test_request(request, response, cx));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        if let Ok(response) = received.try_recv() {
            let CardCommandResult::Brain { output } = response.result.expect("有效动作应成功")
            else {
                panic!("必须是幕僚回执")
            };
            return serde_json::from_str(&output).unwrap();
        }
        assert!(std::time::Instant::now() < deadline, "端点必须有界返回");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
