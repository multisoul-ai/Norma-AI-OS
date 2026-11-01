use super::*;
use gpui::TestAppContext;
use std::sync::{Arc, Mutex};

struct RecordPty(Arc<Mutex<Vec<u8>>>);
impl terminal::PtyBackend for RecordPty {
    fn write(&mut self, bytes: &[u8]) -> Result<(), terminal::PtyError> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(())
    }
    fn resize(&mut self, _: u16, _: u16) -> Result<(), terminal::PtyError> {
        Ok(())
    }
    fn kill(&mut self) {}
}

type Fixture = (
    gpui::Entity<MultisoulApp>,
    gpui::VisualTestContext,
    Arc<Mutex<Vec<u8>>>,
    tempfile::TempDir,
    TestAppContext,
);

/// 隔离项目 + 记录型 PTY 的 App；所有卡片写入的字节汇总到同一份记录。
fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let received = Arc::new(Mutex::new(Vec::new()));
    let captured = received.clone();
    let mut test = TestAppContext::single();
    let (entity, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(move |_, _, _, _| Ok(Box::new(RecordPty(captured.clone())))),
            paths,
        )
    });
    entity.update(cx, |app, cx| app.enter_canvas_for_test(dir.path(), cx));
    (entity, cx.clone(), received, dir, test)
}

fn sent(received: &Arc<Mutex<Vec<u8>>>) -> String {
    String::from_utf8_lossy(&received.lock().unwrap()).into_owned()
}

fn ask(
    app: &mut MultisoulApp,
    window: &mut gpui::Window,
    target: &str,
    message: &str,
) -> Result<String, ui::i18n::LocalizedText> {
    let area = app.mobile_canvas_area(Some(window));
    let zoom = app.runtime.document.viewport.zoom;
    app.ask_agent(target, message, area, zoom)
}

fn agent_cards(app: &MultisoulApp) -> usize {
    app.runtime
        .document
        .nodes()
        .iter()
        .filter(|node| matches!(node.kind, canvas::CanvasNodeKind::Agent(_)))
        .count()
}

/// 场景：`ask codex …` 新建卡片，卡片仍在启动期间任务不能丢，就绪后也不能重复。
/// 执行：ask → 启动期间排空事件 → 标记就绪 → 再排空。
/// 预期：恰好 1 张卡；启动期间 PTY 未收到任务；就绪后任务完整出现且只出现 1 次。
#[test]
fn ask_profile_creates_card_and_delivers_task_once_after_startup() {
    let (entity, mut cx, received, _dir, _test) = fixture();
    let node = entity.update_in(&mut cx, |app, window, _| {
        ask(app, window, "codex", "整理 README 的安装步骤").expect("ask 应被接受");
        assert_eq!(agent_cards(app), 1);
        app.runtime
            .document
            .nodes()
            .iter()
            .find(|n| matches!(n.kind, canvas::CanvasNodeKind::Agent(_)))
            .unwrap()
            .id
    });
    cx.run_until_parked();
    assert!(
        !sent(&received).contains("整理 README"),
        "启动期间不应提前写入：{}",
        sent(&received)
    );
    entity.update(&mut cx, |app, _| {
        app.pending_agent_submissions.mark_ready(node)
    });
    // 就绪事件先输入任务，帧循环再排空到期的回车；用远期时间点模拟多帧。
    entity.update(&mut cx, |app, cx| {
        app.submit_pending_agent_task_for_ready_node(node, cx)
    });
    for tick in 1..=4u64 {
        entity.update(&mut cx, |app, cx| {
            let later = std::time::Instant::now() + std::time::Duration::from_secs(10 * tick);
            app.submit_due_pending_agent_tasks(later, cx)
        });
    }
    cx.run_until_parked();
    let text = sent(&received);
    assert_eq!(
        text.matches("整理 README 的安装步骤").count(),
        1,
        "任务必须完整且仅一次：{text}"
    );
    assert_eq!(entity.update(&mut cx, |app, _| agent_cards(app)), 1);
}

/// 场景：`ask <已有名称> …` 向已有卡片派发。预期：不新建卡，任务送达一次。
#[test]
fn ask_existing_agent_dispatches_once_without_new_card() {
    let (entity, mut cx, received, _dir, _test) = fixture();
    entity.update_in(&mut cx, |app, window, _| {
        crate::app::execute_agent_card_command(
            app,
            window,
            norma::protocol::CardCommand::CreateAgent {
                name: Some("Alan".into()),
                profile: norma::protocol::AgentProfile::Codex,
                activate: true,
            },
        )
        .unwrap();
        let node = app
            .agent_roster_for_provider()
            .into_iter()
            .find(|e| e.name == "Alan")
            .unwrap()
            .active_node_id
            .unwrap();
        app.pending_agent_submissions.mark_ready(node);
        ask(app, window, "alan", "跑一遍单测").expect("已有 Agent 应接受");
        assert_eq!(agent_cards(app), 1);
    });
    cx.run_until_parked();
    assert_eq!(
        sent(&received).matches("跑一遍单测").count(),
        1,
        "{}",
        sent(&received)
    );
}

/// 场景：`ask <未知名称> …`。预期：报错（含名称）、不建卡、不写任何输入。
#[test]
fn ask_unknown_name_errors_without_creating_card() {
    let (entity, mut cx, received, _dir, _test) = fixture();
    let error = entity.update_in(&mut cx, |app, window, _| {
        let error = ask(app, window, "Nobody", "hello").expect_err("未知名称必须报错");
        assert_eq!(agent_cards(app), 0);
        error
    });
    cx.run_until_parked();
    assert!(error.to_string().contains("Nobody"), "{error}");
    assert!(sent(&received).is_empty());
}

/// 场景：Cmd+K 普通消息交给幕僚后，幕僚的 report 回复（语音关闭时同样）必须出现在 Dock 回复区。
/// 预期：未交接时 report 不占用 Dock；交接后第一条 report 显示为 Dock 回复，之后的 report 不再覆盖。
#[test]
fn brain_reply_after_dock_handoff_is_visible_in_dock() {
    let (entity, mut cx, _received, _dir, _test) = fixture();
    entity.update_in(&mut cx, |app, window, cx| {
        app.ensure_command_input(window, cx);
        assert!(
            !app.deliver_dock_brain_reply("无关播报", cx),
            "未交接时不应占用 Dock"
        );
        app.finish_brain_handoff(window, cx);
        assert!(app.deliver_dock_brain_reply("今天有 2 个任务完成", cx));
        assert!(
            !app.deliver_dock_brain_reply("后续播报", cx),
            "只认领一次回复"
        );
        let reply = app.command_dock.as_ref().unwrap().reply.as_ref().unwrap();
        assert_eq!(reply.to_string(), "今天有 2 个任务完成");
        assert!(
            app.command_dock.as_ref().unwrap().visible,
            "回复区需要 Dock 展开才会绘制"
        );
    });
}
