use super::*;
use brain::acp::trace::{AcpTrace, TraceItem, TraceKind};
use brain::{BrainStatus, Trigger};
use gpui::TestAppContext;

/// 场景：项目已打开但本地服务不可用时，幕僚不能误报“请打开工作空间”。
///
/// 数据构造：1 个已打开的临时项目，0 个注册的 Norma endpoint；模拟另一进程占用入口。
/// 执行过程：设置真实项目 root 与身份 → 调用生产 route 注册 → 入口缺失导致注册失败 → 发送幕僚消息。
/// 预期结果：项目仍打开；提示服务连接不可用，而非缺少工作空间；消息不得进入未认证队列。
#[test]
fn opened_project_without_endpoint_reports_connection_failure() {
    let dir = tempfile::tempdir().expect("isolated opened project");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut test = TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    source.update(cx, |app, cx| {
        app.runtime.set_workspace_root(dir.path().to_owned());
        app.workspace_id = Some("opened-project".into());
        app.register_loaded_project_route(None, cx);
        let result = app.push_staff_event(BrainEventKind::StaffRequest {
            text: "帮我检查这个项目".into(),
        });
        assert_eq!(
            app.runtime.workspace_root(),
            Some(dir.path()),
            "route 失败不得关闭已打开的项目"
        );
        assert_eq!(
            result,
            Err("staff.connection_unavailable"),
            "已打开项目应报告连接失败，不能要求重新打开工作空间"
        );
        assert_ne!(
            result,
            Err("island.staff_no_workspace"),
            "未注册 route 不等于未打开项目"
        );
        assert!(
            app.brain.pending_events.borrow().is_empty(),
            "连接失败不得排入无身份的消息"
        );
    });
}

/// 事件入队时固定当前 route，重启后不能将旧队列重绑到新项目。
/// 数据：同一App先用 alpha/old 排队，再切换为 alpha/new，重放旧事件并接收新事件。
/// 执行：调用生产 push_brain/push_scoped_brain，再给状态通道发送旧、新 route 的 trace。
/// 预期：旧事件不重排；新事件保持新身份；旧 trace 不污染新 route 的活动来源。
#[test]
fn queued_event_and_status_cannot_be_rebound_to_a_new_route() {
    let dir = tempfile::tempdir().expect("隔离App数据");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut test = TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    source.update(cx, |app, cx| {
        app.workspace_id = Some("alpha".into());
        app.workspace_route_id = Some("old".into());
        app.push_brain(BrainEventKind::UserRequest {
            text: "old request".into(),
        });
        let old = app
            .brain
            .pending_events
            .borrow_mut()
            .pop_front()
            .expect("旧事件已排队");
        assert_eq!(old.scope.route_id, "old", "入队已捕获原route");
        app.workspace_route_id = Some("new".into());
        app.push_scoped_brain(old);
        assert!(
            app.brain.pending_events.borrow().is_empty(),
            "过期队列不重绑新route"
        );
        app.push_brain(BrainEventKind::UserRequest {
            text: "new request".into(),
        });
        let new = app
            .brain
            .pending_events
            .borrow_mut()
            .pop_front()
            .expect("新事件可排队");
        assert_eq!(new.scope.route_id, "new", "新事件使用当前route");
        let (tx, status) = std::sync::mpsc::channel();
        app.install_brain_host_for_test(super::super::BrainHost {
            service: None,
            status,
            token: "test".into(),
        });
        for (route, turn) in [("new", 7), ("old", 8)] {
            tx.send(BrainStatus {
                scope: BrainScope {
                    workspace_id: "alpha".into(),
                    route_id: route.into(),
                },
                kind: BrainStatusKind::Trace(AcpTrace {
                    at: std::time::SystemTime::now(),
                    lane: "Main".into(),
                    turn,
                    trigger: Trigger::unknown(),
                    item: TraceItem::new(TraceKind::Prompt, "", "request"),
                }),
            })
            .expect("发送原样状态");
        }
        app.drain_brain_status(cx);
        assert_eq!(
            app.brain.last_turn.get("Main"),
            Some(&7),
            "较迟到达的旧route第8轮不能覆盖新route第7轮"
        );
    });
}

/// 场景：岛内全局对话经锚点宿主进入唯一大脑，未启用或没有身份时给出本地化原因而不是静默丢弃。
/// 数据：宿主有 alpha/route 身份；先无设置、再设置为未启用、最后启用。
/// 预期：依次 Err(island.staff_loading)、Err(island.staff_disabled)；启用后 Ok 且排入的事件带 alpha/route 身份；
/// 没有 workspace 身份的宿主返回 Err(island.staff_no_workspace)，队列保持为空。
#[test]
fn staff_requests_report_why_the_brain_is_unavailable() {
    let dir = tempfile::tempdir().expect("隔离App数据");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut test = TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    source.update(cx, |app, _| {
        let request = || BrainEventKind::UserRequest {
            text: "汇总".into(),
        };
        assert_eq!(
            app.push_staff_event(request()),
            Err("island.staff_no_workspace")
        );
        app.workspace_id = Some("alpha".into());
        app.workspace_route_id = Some("route".into());
        assert_eq!(app.push_staff_event(request()), Err("island.staff_loading"));
        app.brain.settings = Some(Default::default());
        assert_eq!(
            app.push_staff_event(request()),
            Err("island.staff_disabled")
        );
        assert!(
            app.brain.pending_events.borrow().is_empty(),
            "不可用时不得排队"
        );
        app.brain.settings = Some(super::super::config::BrainSettings {
            enabled: true,
            ..Default::default()
        });
        assert_eq!(app.push_staff_event(request()), Ok(()));
        let event = app
            .brain
            .pending_events
            .borrow_mut()
            .pop_front()
            .expect("已排队");
        assert_eq!(
            event.scope.workspace_id, "alpha",
            "身份来自锚点宿主的精确 workspace/route"
        );
        assert_eq!(event.scope.route_id, "route");
    });
}

/// 关闭 Brain 后 Worker 的终态仍须排队落盘，普通对话不应触发模型。
/// 数据构造：alpha/route，先有存活服务；绑定 Worker node=7、dispatch=d，另有普通 UserRequest。
/// 执行过程：服务存活时投递 Worker 失败 → 立即停用 → 普通对话被拒 → 后台保存后重载快照。
/// 预期结果：只有原 Worker 的 Failed 持久化；模型保持关闭，没有新对话或 TaskRequest。
#[test]
fn disabled_brain_preserves_worker_completion_for_background_persistence() {
    let dir = tempfile::tempdir().expect("isolated App");
    let mut config = brain::BrainConfig::new(
        brain::Backend {
            kind: brain::BackendKind::ClaudeCode,
            program: "/must-not-launch".into(),
            args: Vec::new(),
        },
        "/opt/norma".into(),
        dir.path().into(),
        dir.path().join("staff"),
    );
    config.router.summary_every = None;
    let (service, status) = brain::BrainService::start(config);
    let store = service.tasks();
    let mut task = norma::protocol::Task::new("task", "alpha", "project", "Work");
    task.workers.push(norma::protocol::Worker::new(
        "Simon", "Claude", "Build", 7, "d",
    ));
    store.create(task).expect("bound Worker");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut test = TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    source.update(cx, |app, cx| {
        app.workspace_id = Some("alpha".into());
        app.workspace_route_id = Some("route".into());
        app.install_brain_host_for_test(super::super::BrainHost { service: Some(service), status, token: "test".into() });
        app.brain.settings = Some(super::super::config::BrainSettings { enabled: true, ..Default::default() });
        app.push_brain(BrainEventKind::AgentFinished {
            node_id: 7, dispatch_id: Some("d".into()), agent: "Simon".into(),
            outcome: brain::Outcome::Failed, excerpt: "launch failed".into(), from_result: true,
        });
        app.brain.retire_brain_for_test();
        app.brain.settings = Some(Default::default());
        app.push_brain(BrainEventKind::UserRequest { text: "do not run".into() });
        let pending = app.staff.events.borrow();
        assert_eq!(pending.len(), 1, "disabled Brain must retain only Worker settlement");
        assert_eq!(pending[0].scope.workspace_id, "alpha", "settlement keeps original workspace");
        assert!(matches!(&pending[0].kind, BrainEventKind::AgentFinished { node_id: 7, dispatch_id: Some(id), .. } if id == "d"), "exact node and dispatch must survive");
        assert!(!pending.iter().any(|event| matches!(event.kind, BrainEventKind::UserRequest { .. })), "disabled Brain must not enqueue new model work");
        drop(pending);
        app.save_staff_events(store.clone(), cx);
    });
    cx.run_until_parked();
    let saved = brain::staff::TaskStore::new(dir.path().join("staff"))
        .get("alpha", "task")
        .expect("durable settlement");
    assert_eq!(
        saved.workers[0].state,
        norma::protocol::WorkerState::Failed,
        "disabled service must not leave Worker running"
    );
    assert_eq!(
        saved.messages.len(),
        1,
        "one failure must be recorded exactly once"
    );
    source.update(cx, |app, _| {
        assert!(
            app.brain.pending_events.borrow().is_empty(),
            "persistence must not wake the disabled model"
        );
        assert!(
            app.staff.recorded.borrow().is_empty(),
            "disabled model must not retain later wakeups"
        );
        assert!(
            !app.staff.saving_event,
            "background write must release its active slot"
        );
    });
}
