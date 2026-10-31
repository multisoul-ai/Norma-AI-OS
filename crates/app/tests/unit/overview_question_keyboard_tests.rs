#[path = "../support/brain_question.rs"]
mod support;
use super::*;
use gpui::{
    div, prelude::*, AppContext, Entity, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers,
    PlatformInput, Render, TestAppContext,
};
use std::{cell::Cell, rc::Rc};

struct Harness {
    app: Entity<MultisoulApp>,
    row: OverviewQuestion,
    outer_keys: Rc<Cell<usize>>,
}
impl Render for Harness {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let keys = self.outer_keys.clone();
        div()
            .id("question-keyboard-parent")
            .on_key_down(move |event: &KeyDownEvent, _, _| {
                if event.keystroke.key == "enter" {
                    keys.set(keys.get() + 1);
                }
            })
            .child(
                self.app
                    .update(cx, |app, cx| app.render_question_row(&self.row, 0, cx)),
            )
    }
}

fn key_cycle(cx: &mut gpui::VisualTestContext, key: &str) {
    let stroke = Keystroke::parse(key).unwrap();
    cx.update(|window, cx| {
        window.dispatch_event(
            PlatformInput::KeyDown(KeyDownEvent {
                keystroke: stroke.clone(),
                is_held: false,
            }),
            cx,
        );
        window.draw(cx).clear();
        window.dispatch_event(PlatformInput::KeyUp(KeyUpEvent { keystroke: stroke }), cx);
        window.draw(cx).clear();
    });
}

/// 场景：真实控件的一次按下/抬起只能切换一次多选，问答按钮不激活外层概览动作。
/// 构造：生产问答卡片、两题及一个统计Enter冒泡的外层容器；隔离App和数据库。
/// 执行：Tab聚焦编辑按钮，发送完整Enter；聚焦多选后发送Space，再自定义输入字符与空格。
/// 预期：编辑被打开但外层动作零次；多选仅切换一次；自定义文本正常插入。
#[test]
fn overview_question_keyboard_is_single_activation_and_isolated() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let mut test = TestAppContext::single();
    test.update(ui::init_components);
    let mut source = None;
    let mut owner = None;
    let mut question_target = None;
    let outer_keys = Rc::new(Cell::new(0));
    let (_, cx) = test.add_window_view(|window, cx| {
        let app = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                paths.clone(),
            )
        });
        source = Some(app.clone());
        let project = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                paths,
            )
        });
        let row = project.update(cx, |app, cx| {
            let mut raw: serde_json::Value = serde_json::from_str(include_str!(
                "../../../norma/tests/fixtures/questions/claude-pre-tool-use.json"
            ))
            .unwrap();
            let mut multiple = raw["tool_input"]["questions"][0].clone();
            multiple["question"] = "Choose multiple colors".into();
            multiple["multiSelect"] = true.into();
            raw["tool_input"]["questions"]
                .as_array_mut()
                .unwrap()
                .push(multiple);
            let (target, _, _) = support::make_question_from_raw(app, cx, dir.path(), raw);
            question_target = Some(target);
            app.question_overview_rows("a").remove(0)
        });
        app.update(cx, |app, _| {
            app.desktop_workspace.active_id = Some("w".into());
            app.desktop_workspace
                .sessions
                .insert(("w".into(), "a".into()), project.clone());
        });
        owner = Some(project);
        let harness = cx.new(|cx| {
            cx.observe(&app, |_, _, cx| cx.notify()).detach();
            Harness {
                app,
                row,
                outer_keys: outer_keys.clone(),
            }
        });
        ui::component::Root::new(harness, window, cx)
    });
    let source = source.unwrap();
    cx.update(|window, cx| window.draw(cx).clear());
    // The disclosure row is the first tab stop; opening it does not start editing.
    cx.update(|window, cx| {
        window.blur();
        window.focus_next();
        window.draw(cx).clear();
    });
    key_cycle(cx, "enter");
    assert!(
        source.read_with(cx, |app, _| app.desktop_workspace.attention.open.is_some()),
        "Enter 展开原题"
    );
    assert!(
        source.read_with(cx, |app, _| app.desktop_question_editor.is_none()),
        "展开不应自动采用或编辑建议"
    );
    key_cycle(cx, "space");
    assert!(
        source.read_with(cx, |app, _| app.desktop_workspace.attention.open.is_none()),
        "一次完整 Space 只能收起一次，不能被原生 key-up 再次展开"
    );
    key_cycle(cx, "enter");
    assert!(
        source.read_with(cx, |app, _| app.desktop_workspace.attention.open.is_some()),
        "Enter 再次展开且不发生双切换"
    );
    cx.update(|window, cx| {
        window.focus_next();
        window.draw(cx).clear();
    });
    key_cycle(cx, "enter");
    assert!(
        source.read_with(cx, |app, _| app.desktop_question_editor.is_some()),
        "Enter激活生产编辑按钮"
    );
    assert_eq!(outer_keys.get(), 0, "问答按钮回车不能激活外层概览行");
    let bounds = cx
        .debug_bounds("question-option-a-1-0-1-option-0")
        .expect("多选控件应可见");
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.update(|window, cx| window.draw(cx).clear());
    let selected = |cx: &mut gpui::VisualTestContext| {
        source.read_with(cx, |app, cx| {
            app.desktop_question_editor.as_ref().unwrap().answers(cx)[1]
                .selected
                .clone()
        })
    };
    assert_eq!(selected(cx), ["option-0"], "鼠标先选中多选项");
    // The pinned components prevent mouse focus; Tab traverses three radios before the checkbox.
    cx.update(|window, cx| {
        window.blur();
        for _ in 0..5 {
            window.focus_next();
        }
        window.draw(cx).clear();
    });
    key_cycle(cx, "space");
    assert!(
        selected(cx).is_empty(),
        "一次Space只取消一次，不能双回调恢复选中"
    );
    let custom = cx.debug_bounds("question-option-a-1-0-0-custom").unwrap();
    cx.simulate_click(custom.center(), Modifiers::none());
    cx.update(|window, cx| {
        window.draw(cx).clear();
        window.blur();
        // Three radios precede the newly visible custom text input.
        for _ in 0..5 {
            window.focus_next();
        }
        window.draw(cx).clear();
    });
    cx.simulate_keystrokes("a space b");
    assert_eq!(
        source.read_with(cx, |app, cx| app
            .desktop_question_editor
            .as_ref()
            .unwrap()
            .answers(cx)[0]
            .text
            .clone()),
        Some("a b".into()),
        "概览事件隔离不能吞掉自由文本及空格"
    );
    // A cached enabled row must not open editing after the delegate has claimed the original.
    let target = question_target.unwrap();
    let owner = owner.unwrap();
    let answers = source.read_with(cx, |app, cx| {
        app.desktop_question_editor.as_ref().unwrap().answers(cx)
    });
    let mut answers = answers;
    answers[1].selected = vec!["option-1".into()];
    owner.update(cx, |app, cx| {
        app.authorize_question_delegation(&target).unwrap();
        app.answer_question_from_brain(target, answers, cx).unwrap();
    });
    source.update(cx, |app, cx| {
        app.desktop_question_editor = None;
        cx.notify();
    });
    cx.update(|window, cx| {
        window.draw(cx).clear();
        window.blur();
        window.focus_next();
        window.focus_next();
        window.draw(cx).clear();
    });
    key_cycle(cx, "enter");
    assert!(
        source.read_with(cx, |app, _| app.desktop_question_editor.is_none()),
        "模型已占有原题后，旧帧编辑点击必须拒绝，不能一边编辑一边后台送答"
    );
}
