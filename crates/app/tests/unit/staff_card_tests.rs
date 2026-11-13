use super::*;
use gpui::{div, prelude::*, px, size, AppContext, Render, TestAppContext, Window};
use norma::protocol::{
    QuestionItem, QuestionOption, StaffQuestion, StaffQuestionState, TaskMessage,
};

struct CardHost {
    app: Entity<MultisoulApp>,
    task: Task,
}
impl Render for CardHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let task = self.task.clone();
        div().size_full().child(self.app.update(cx, |app, cx| {
            app.ensure_staff_question_editors(&task, window, cx);
            app.render_message(&task, 0, &task.messages[0], window, cx)
        }))
    }
}

/// 场景：问答卡四个状态只允许待答卡编辑；终态保留题目和结果。
/// 数据构造：一个带说明、自定义答案的单选题；依次设为Pending/Answered/Skipped/TimedOut。
/// 执行过程：生产渲染器真实布局；预期：四态有可见卡片，只有待答态出现选项与提交，终态无提交按钮。
#[test]
fn staff_question_cards_render_each_state_and_remove_terminal_actions() {
    for state in [
        StaffQuestionState::Pending,
        StaffQuestionState::Answered,
        StaffQuestionState::Skipped,
        StaffQuestionState::TimedOut,
    ] {
        let dir = tempfile::tempdir().expect("isolated project");
        let paths = storage::DatabasePaths::isolated_test(dir.path());
        let mut question = StaffQuestion::new(
            "q".into(),
            vec![QuestionItem {
                key: "theme".into(),
                provider_id: None,
                prompt: "Which theme?".into(),
                options: vec![QuestionOption {
                    key: "dark".into(),
                    label: "Dark".into(),
                    description: Some("Comfortable at night".into()),
                }],
                multiple: false,
                allow_custom: true,
                secret: false,
            }],
            None,
        );
        question.state = state;
        let mut task = Task::new("t", "w", "p", "Theme");
        task.messages.push(TaskMessage::Question {
            question: Box::new(question),
        });
        let mut test = TestAppContext::single();
        let (_, cx) = test.add_window_view(|_, cx| {
            let app = cx.new(|cx| {
                MultisoulApp::new_with_pty_factory_and_database_paths(
                    cx,
                    Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                    paths,
                )
            });
            CardHost { app, task }
        });
        cx.simulate_resize(size(px(800.), px(600.)));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
        let card = cx
            .debug_bounds("staff-question-q")
            .expect("question bounds");
        assert!(
            card.size.height > px(40.),
            "四种状态的题目和状态必须可见：{state:?}"
        );
        assert_eq!(
            cx.debug_bounds("staff-question-submit-q").is_some(),
            state == StaffQuestionState::Pending,
            "只有待答卡可以提交：{state:?}"
        );
        assert_eq!(
            cx.debug_bounds("staff-question-option-q-0-0").is_some(),
            state == StaffQuestionState::Pending,
            "终态不能残留可编辑选项：{state:?}"
        );
    }
}

/// 场景：忽略只折叠目标提议，展开后可恢复，较新提议存在时不显示恢复操作。
/// 数据构造：提议1标为ignored，分别折叠/展开，并在第三轮追加提议2。
/// 执行过程：调用生产渲染器布局；预期：始终有忽略行、无采用按钮，仅未被替代且展开时有恢复按钮。
#[test]
fn staff_ignored_proposal_collapses_and_only_latest_can_restore() {
    for (expanded, newer) in [(false, false), (true, false), (true, true)] {
        let dir = tempfile::tempdir().expect("isolated project");
        let paths = storage::DatabasePaths::isolated_test(dir.path());
        let mut task = Task::new("t", "w", "p", "Proposal");
        task.proposals.push(norma::protocol::StaffProposal {
            id: 1,
            agent: "Worker".into(),
            title: "Inspect".into(),
            prompt: "Read project".into(),
            why: "Needed".into(),
            approved: false,
            ignored: true,
            dispatch_id: None,
        });
        if newer {
            let mut next = task.proposals[0].clone();
            next.id = 2;
            next.ignored = false;
            task.proposals.push(next);
        }
        task.messages.push(TaskMessage::Proposal { proposal_id: 1 });
        let mut test = TestAppContext::single();
        let (_, cx) = test.add_window_view(|_, cx| {
            let app = cx.new(|cx| {
                let mut app = MultisoulApp::new_with_pty_factory_and_database_paths(
                    cx,
                    Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                    paths,
                );
                if expanded {
                    app.staff.expanded.insert("proposal-t-1".into());
                }
                app
            });
            CardHost { app, task }
        });
        cx.simulate_resize(size(px(800.), px(600.)));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
        assert!(
            cx.debug_bounds("staff-ignored-1").is_some(),
            "忽略行始终可见"
        );
        assert!(
            cx.debug_bounds("staff-adopt-1").is_none(),
            "已忽略提议不能误派发"
        );
        assert_eq!(
            cx.debug_bounds("staff-restore-1").is_some(),
            expanded && !newer,
            "仅展开且无更新提议可恢复"
        );
    }
}
