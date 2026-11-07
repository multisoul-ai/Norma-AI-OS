use super::super::controller::IslandController;
use super::super::question_editor::EditingQuestion;
use super::*;
use crate::app::MultisoulApp;
use crate::workspace::desktop::QuestionEditor;
use gpui::{AppContext, Entity, Render, TestAppContext, Window};
use runtime::island::IslandEvent;
use std::time::{Duration, Instant};

struct QuestionWindow(Entity<IslandController>);
impl Render for QuestionWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// 场景：用户填写好答案后原工作空间释放，岛的迟到提交必须报告失效并保留草稿。
/// 数据：生产问答fixture与自由文本草稿；路由source未注册，模拟宿主已经关闭释放。
/// 执行：创建真实InputState，写入文字，调用岛提交服务。
/// 预期：没有待发送操作，草稿和文字仍在；用户可见失效提示而非假成功。
#[test]
fn island_question_failed_submit_keeps_draft() {
    let temp = tempfile::tempdir().expect("隔离项目");
    let mut test = TestAppContext::single();
    test.update(ui::init_components);
    let route = IslandTarget {
        source: 999,
        workspace: "w".into(),
        area: "a".into(),
    };
    let (view, cx) = test.add_window_view(|window, cx| {
        let app = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                storage::DatabasePaths::isolated_test(&temp.path().join("data")),
            )
        });
        let row = app.update(cx, |app, cx| {
            let raw = serde_json::from_str(include_str!(
                "../../../norma/tests/fixtures/questions/claude-pre-tool-use.json"
            ))
            .expect("问答fixture");
            crate::test_brain_question::make_question_from_raw(app, cx, temp.path(), raw);
            app.question_overview_rows("a").remove(0)
        });
        let model = super::super::model(cx);
        let controller = cx.new(|cx| IslandController::new(model, cx));
        controller.update(cx, |state, cx| {
            let mut editor = QuestionEditor::new(&row, window, cx);
            editor.toggle_custom(0);
            editor
                .input(0)
                .update(cx, |input, cx| input.set_value("Cobalt blue", window, cx));
            state.editing.insert(
                "draft".into(),
                EditingQuestion::mount(route.clone(), editor, 11, window, cx),
            );
        });
        QuestionWindow(controller)
    });
    view.update(cx, |view, cx| {
        view.0.update(cx, |state, cx| {
            let editing = state.editing.get("draft").expect("提交前应有草稿");
            let target = editing.editor.target().clone();
            let answers = editing.editor.answers(cx);
            state.submit_question(route.clone(), target, answers.clone(), cx);
            assert!(state.pending.is_empty(), "失效宿主不能假装开始投递");
            assert_eq!(
                state
                    .editing
                    .get("draft")
                    .expect("失败不能丢草稿")
                    .editor
                    .answers(cx),
                answers,
                "文字必须保留"
            );
            assert_eq!(
                state.message.as_ref().map(ToString::to_string),
                Some(text("island.expired").into()),
                "必须显示失效提示"
            );
        })
    });
}

/// 场景：持久化完成早于原Hook写出，岛必须等待真实传输而非提前显示失败。
/// 数据：隔离来源项目、原生Claude问题、精确路由、已选择的答案及上次提交的成功提示。
/// 执行：经岛提交后停在Hook Ready，再模拟发送完成并推进异步任务。
/// 预期：Ready阶段仍在发送且不报失败；传输完成后清除pending、显示已提交，原题只投递一次。
#[test]
fn island_question_waits_for_transport_before_reporting_result() {
    let temp = tempfile::tempdir().expect("隔离项目");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let mut test = TestAppContext::single();
    test.update(ui::init_components);
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths.clone(),
        )
    });
    let (target, ticket, answers) = source.update(cx, |app, cx| {
        let raw = serde_json::from_str(include_str!(
            "../../../norma/tests/fixtures/questions/claude-pre-tool-use.json"
        ))
        .expect("原生问题fixture");
        crate::test_brain_question::make_question_from_raw(app, cx, temp.path(), raw)
    });
    let host = cx.new(|cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    host.update(cx, |app, _| {
        app.desktop_workspace
            .sessions
            .insert(("w".into(), "a".into()), source.clone());
    });
    cx.update(|_, cx| super::super::actions::register_source(&host, cx));
    let route = IslandTarget {
        source: host.entity_id().as_u64(),
        workspace: "w".into(),
        area: "a".into(),
    };
    let model = cx.update(|_, cx| super::super::model(cx));
    let controller = cx.new(|cx| IslandController::new(model, cx));
    controller.update(cx, |state, cx| {
        state.message = Some(ui::i18n::LocalizedText::new("island.reply_submitted"));
        state.event(11, IslandEvent::Expand, cx);
        state.event(11, IslandEvent::InputFocusChanged(true), cx);
        state.focused_question = Some((
            super::super::controller::alert_key(
                route.source,
                &route.workspace,
                &route.area,
                &target,
            ),
            controller.entity_id(),
        ));
        state.submit_question(route.clone(), target.clone(), answers, cx)
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while ticket.status(Instant::now()) != norma::agent_hook::QuestionHookStatus::Ready {
        cx.run_until_parked();
        assert!(Instant::now() < deadline, "后台准备必须完成");
        std::thread::sleep(Duration::from_millis(1));
    }
    cx.run_until_parked();
    controller.read_with(cx, |state, _| {
        assert_eq!(
            state.pending.len(),
            1,
            "排队不等于实际发送，必须保持pending"
        );
        assert!(state.message.is_none(), "尚未送达不能提前报告成功或失败");
    });
    assert!(
        ticket.take_response(Instant::now()).is_some(),
        "原Hook恰有一份待写答案"
    );
    ticket
        .with_response_write(Instant::now(), || Ok(true))
        .expect("模拟传输完成");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(25));
        if controller.read_with(cx, |state, _| state.pending.is_empty()) {
            break;
        }
        assert!(Instant::now() < deadline, "发送回执必须有界反映到岛");
        std::thread::sleep(Duration::from_millis(1));
    }
    controller.read_with(cx, |state, _| {
        assert_eq!(
            state.message.as_ref().map(ToString::to_string),
            Some(text("island.reply_submitted").into()),
            "只有实际传输后显示已提交"
        );
        assert!(
            state.focused_question.is_none(),
            "表单先消失也必须释放该问题的输入焦点"
        );
    });
    controller.update(cx, |state, cx| {
        state.event(11, IslandEvent::PointerLeft, cx);
        assert!(
            state.displays.state.deadline_ms().is_some(),
            "提交后离开应恢复自动收回，不能被已消失输入框锁住"
        );
    });
    assert!(
        ticket.take_response(Instant::now()).is_none(),
        "轮询回执不得重复投递"
    );
}
