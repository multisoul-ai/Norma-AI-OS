use super::*;
use gpui::{Render, TestAppContext};

struct DraftWindow(Option<QuestionEditor>);
impl Render for DraftWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// 场景：用户只填写了部分题目，显示器关闭后迁移编辑器到另一窗口。
/// 数据：两道必答题，第一题只填自由文本，第二题留空，整体答案仍无效。
/// 执行：在窗口A创建输入，写入草稿；在窗口B重新挂载并读取答案及输入实体身份。
/// 预期：文字与选项不丢失，未答题仍未答；输入Entity必须重建以重新注册窗口焦点监听。
#[test]
fn island_question_remount_preserves_incomplete_draft_and_rebinds_inputs() {
    let mut test = TestAppContext::single();
    test.update(ui::init_components);
    let row = OverviewQuestion {
        area_id: "a".into(),
        agent: "Iris".into(),
        available: true,
        delegated: false,
        can_delegate: false,
        suggestion: None,
        snapshot: norma::protocol::QuestionSnapshot {
            question: super::super::tests::question(),
            state: norma::protocol::QuestionState::Pending,
            delivery_id: None,
            error: None,
            evidence: None,
        },
    };
    let (first, cx) =
        test.add_window_view(|window, cx| DraftWindow(Some(QuestionEditor::new(&row, window, cx))));
    let (editor, original_input, answers) = first.update_in(cx, |view, window, cx| {
        let editor = view.0.as_mut().expect("窗口A应有编辑器");
        editor.toggle_custom(0);
        editor.input(0).update(cx, |input, cx| {
            input.set_value("unfinished reply", window, cx)
        });
        let answers = editor.answers(cx);
        assert!(
            row.snapshot.question.validate_answers(&answers).is_err(),
            "第二题未答时必须是不完整草稿"
        );
        (
            view.0.take().expect("取走编辑器"),
            answers[0].text.clone().expect("已写入自由文本"),
            answers,
        )
    });
    let original_entity = editor.input(0).entity_id();
    let (second, cx) =
        test.add_window_view(|window, cx| DraftWindow(Some(editor.remount(window, cx))));
    second.read_with(cx, |view, cx| {
        let editor = view.0.as_ref().expect("窗口B应恢复编辑器");
        assert_eq!(
            editor.answers(cx),
            answers,
            "迁移必须保留不完整草稿及其顺序"
        );
        assert_ne!(
            editor.input(0).entity_id(),
            original_entity,
            "不能复用绑定窗口A的输入控件"
        );
        assert_eq!(original_input, "unfinished reply", "迁移前必须真正写入文字");
    });
}
