use super::*;
use brain::acp::trace::{AcpTrace, TraceItem, TraceKind};
use brain::{BrainStatus, Trigger};
use gpui::TestAppContext;

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
