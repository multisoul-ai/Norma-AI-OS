use super::*;
use gpui::{div, prelude::*, px, size, AppContext, Render, TestAppContext, Window};
use norma::protocol::{QuestionItem, QuestionOption, StaffQuestion, StaffQuestionState};

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
        task.attach_card(norma::protocol::StaffPart::Question(question.id.clone()));
        task.questions.push(question);
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
            model: None,
            round: 0,
            auto: false,
            downgrade: None,
            superseded: false,
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
        task.attach_card(norma::protocol::StaffPart::Proposal(1));
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

fn proposal_task(prompt: &str) -> Task {
    let mut task = Task::new("t", "w", "p", "Proposal");
    task.proposals.push(norma::protocol::StaffProposal {
        id: 1,
        agent: "Worker".into(),
        title: "做单页贪吃蛇".into(),
        prompt: prompt.into(),
        why: "浏览器直接玩".into(),
        model: None,
        round: 0,
        auto: false,
        downgrade: None,
        superseded: false,
        approved: false,
        ignored: false,
        dispatch_id: None,
    });
    task.attach_card(norma::protocol::StaffPart::Proposal(1));
    task
}

/// 场景：方案A建议卡，长指令默认收起，展开后完整显示；底栏控件全部在卡片内同一行。
/// 数据构造：8段指令（长）与一句指令（短）；800×600窗口；展开键 proposal-prompt-t-1。
/// 执行过程：生产渲染器真实布局，分别读取指令面板、展开按钮、底栏控件与卡片边界。
/// 预期结果：长指令收起≤132px、展开后更高；短指令无展开按钮；采用/忽略/修改/智能体在卡内且同一行。
#[test]
fn staff_proposal_card_collapses_long_prompt_and_keeps_footer_inside() {
    let long = (1..=8)
        .map(|n| format!("第{n}段：验收标准与假设说明，需要完整保留给执行智能体。"))
        .collect::<Vec<_>>()
        .join("\n\n");
    for (prompt, expanded) in [
        (long.as_str(), false),
        (long.as_str(), true),
        ("做个贪吃蛇", false),
    ] {
        let dir = tempfile::tempdir().expect("isolated project");
        let paths = storage::DatabasePaths::isolated_test(dir.path());
        let task = proposal_task(prompt);
        let mut test = TestAppContext::single();
        let (_, cx) = test.add_window_view(|_, cx| {
            let app = cx.new(|cx| {
                let mut app = MultisoulApp::new_with_pty_factory_and_database_paths(
                    cx,
                    Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                    paths,
                );
                if expanded {
                    app.staff.expanded.insert("proposal-prompt-t-1".into());
                }
                app
            });
            CardHost { app, task }
        });
        cx.simulate_resize(size(px(800.), px(600.)));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
        let card = cx.debug_bounds("staff-proposal-card-1").expect("建议卡");
        let panel = cx
            .debug_bounds("staff-proposal-prompt-1")
            .expect("指令面板");
        let toggle = cx.debug_bounds("staff-proposal-expand-1");
        if prompt == long {
            assert!(toggle.is_some(), "长指令须有展开/收起按钮");
            assert_eq!(
                panel.size.height <= px(132.),
                !expanded,
                "长指令收起时≤132px，展开后完整显示：{panel:?}"
            );
        } else {
            assert!(toggle.is_none(), "短指令不出现展开按钮");
        }
        let adopt = cx.debug_bounds("staff-adopt-1").expect("采用");
        for name in [
            "staff-ignore-1",
            "staff-edit-proposal-1",
            "staff-agent-pick-1",
        ] {
            let control = cx.debug_bounds(name).expect(name);
            assert!(
                control.bottom() <= card.bottom() && control.top() >= panel.bottom(),
                "{name} 须在卡片底栏内：{control:?} / {card:?}"
            );
            assert!(
                (control.top() - adopt.top()).abs() < px(1.),
                "{name} 与采用按钮同一行：{control:?} / {adopt:?}"
            );
        }
        assert!(adopt.bottom() <= card.bottom(), "采用按钮在卡片内");
        assert!(
            adopt.right() >= cx.debug_bounds("staff-ignore-1").unwrap().right(),
            "主按钮在最右"
        );
    }
}
