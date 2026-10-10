use agent::card_instance::{CardInstanceAssignment, CardInstanceTracking};
use canvas::{AgentProfile, CanvasRect};
use gpui::TestAppContext;
use std::sync::{Arc, Mutex};

pub(crate) type CardWrites = Arc<Mutex<Vec<Vec<u8>>>>;
pub(crate) fn with_cards(
    run: impl FnOnce(
        gpui::Entity<crate::app::MultisoulApp>,
        &mut gpui::VisualTestContext,
        [u64; 2],
        [CardWrites; 2],
    ),
) {
    let writes: [CardWrites; 2] = std::array::from_fn(|_| Arc::new(Mutex::new(Vec::new())));
    let observed = writes.clone();
    let index = std::sync::atomic::AtomicUsize::new(0);
    let factory: runtime::PtyFactory = Box::new(move |_, _, _, tx| {
        let record = observed[index.fetch_add(1, std::sync::atomic::Ordering::Relaxed)].clone();
        Ok(Box::new(terminal::TestPtyBackend::recording(tx, record)))
    });
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new().expect("临时项目");
    let (app, cx) =
        gpui.add_window_view(|_, cx| crate::app::MultisoulApp::new_with_pty_factory(cx, factory));
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    let nodes = app.update(cx, |app, _| {
        std::array::from_fn(|index| {
            app.runtime
                .spawn_agent(
                    AgentProfile::OpenCode,
                    CanvasRect::new(index as f32 * 700.0, 0.0, 640.0, 400.0),
                    80,
                    24,
                )
                .expect("既有卡片")
        })
    });
    run(app, cx, nodes, writes);
}
pub(crate) fn accept(
    app: &mut crate::app::MultisoulApp,
    node: u64,
    id: &str,
    text: &str,
    tracking: CardInstanceTracking,
    definition_id: Option<i64>,
) {
    let target = app
        .runtime
        .workspace_card_target("workspace", "area", 1, node)
        .expect("冻结明确目标");
    app.accept_card_instance_assignment(
        "workspace",
        "area",
        1,
        CardInstanceAssignment {
            assignment_id: id.into(),
            target,
            name_at_acceptance: "Betty".into(),
            definition_id,
            text: text.into(),
            tracking,
            force_untracked: false,
        },
    )
    .expect("实例任务应接收");
}
pub(crate) fn observe(
    app: &mut crate::app::MultisoulApp,
    node: u64,
    name: &str,
    turn: &str,
    prompt: &str,
) -> bool {
    let event = norma::agent_hook::parse_hook_event(norma::agent_hook::HookSource::OpenCode, node, serde_json::json!({"hook_event_name":name,"session_id":"session","turn_id":turn,"prompt":prompt})).expect("可信 Hook 数据");
    let (sender, receiver) = crossbeam_channel::unbounded();
    sender.send(event).expect("Hook 进入生产接收队列");
    let server = crate::settings::agent_hooks::AgentHookServer::for_test_events(receiver);
    let report = server.drain_into_runtime(&mut app.runtime);
    assert!(report.events.len() <= 1, "单项 Hook 不能被生产 drain 复制");
    report
        .events
        .first()
        .is_some_and(|event| app.observe_card_instance_hook(event))
}
