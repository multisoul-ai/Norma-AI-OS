use super::layout_tests::MessageListHost;
use super::*;
use gpui::{px, size, AppContext, TestAppContext};

/// 场景：已派发的提议只保留任务名与智能体卡片名的单行卡片（不再有原话、假设、按钮）。
/// 数据构造：900x700 窗口；含长原话的提议，已有 dispatch_id。
/// 执行过程：渲染生产消息列表一帧，读取卡片 bounds 与采用按钮。
/// 预期结果：卡片存在且为单行高度（<72），没有“采用并派发”按钮。
#[test]
fn staff_dispatched_proposal_is_compact() {
    use norma::protocol::{StaffProposal, TaskMessage};
    let dir = tempfile::tempdir().expect("isolated project");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut task = Task::new("t", "w", "p", "派活");
    task.proposals.push(StaffProposal {
        id: 1,
        round: 0,
        agent: "Ethan".into(),
        title: "派活可选类型和模型".into(),
        prompt: "用户原话：很长的原话。".repeat(20),
        why: "幕僚指派目前选不了".into(),
        model: None,
        approved: true,
        ignored: false,
        dispatch_id: Some("d1".into()),
    });
    task.messages.push(TaskMessage::Proposal { proposal_id: 1 });
    let mut test = TestAppContext::single();
    let (_, cx) = test.add_window_view(|_, cx| {
        let app = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                paths,
            )
        });
        MessageListHost { app, task }
    });
    cx.simulate_resize(size(px(900.), px(700.)));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let card = cx
        .debug_bounds("staff-proposal-card-1")
        .expect("proposal card");
    assert!(card.size.height < px(72.), "应为单行卡片：{card:?}");
    assert!(
        cx.debug_bounds("staff-adopt-1").is_none(),
        "已派发不应再有采用按钮"
    );
}

/// 场景：Norma 长回复（截图中第四行压住下一张卡/输入框）。
/// 数据构造：约 150 字中英混排回复，窗口宽 520/700/900/1100。
/// 执行过程：渲染生产消息列表，读取该消息内容高度，并按字宽估算最少行数。
/// 预期结果：内容高度不小于“最少行数×行高”，即文字不会超出自己的布局盒。
#[test]
fn staff_norma_prose_height_covers_wrapped_lines() {
    use norma::protocol::TaskMessage;
    let dir = tempfile::tempdir().expect("isolated project");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut task = Task::new("t", "w", "p", "派活");
    let text = "Yara 第二次也没启动成功。我先看还有哪些 agent 处于就绪，再告诉你怎么处理。贪吃蛇还是没做出来。Yara 第二次启动也失败了（worker_not_ready），没有可验收的结果，任务仍保持进行中。我不会自动再派给她。请指定另一位 Agent，或等 Yara 稳定后再让我重试一次。";
    task.messages.push(TaskMessage::Norma { text: text.into() });
    task.messages.push(TaskMessage::Reopened);
    let units: f32 = text
        .chars()
        .map(|c| if c.is_ascii() { 7.7 } else { 14. })
        .sum();
    let mut test = TestAppContext::single();
    let (_, cx) = test.add_window_view(|_, cx| {
        let app = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                paths,
            )
        });
        MessageListHost { app, task }
    });
    for width in [520., 700., 900., 1100.] {
        cx.simulate_resize(size(px(width), px(700.)));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
        let body = cx
            .debug_bounds("staff-message-body-0")
            .expect("body bounds");
        let available = f32::from(body.size.width).min(760.);
        let lines = (units / available).ceil();
        let min_height = lines * 14. * 1.7;
        assert!(
            f32::from(body.size.height) + 2. >= min_height,
            "宽 {width}：内容高 {:?} 小于 {lines} 行所需 {min_height}",
            body.size.height
        );
    }
}

/// 场景：长 Norma 说明后紧接待采用建议，滚动区不能压缩正文或卡片。
/// 数据构造：截图中含行内代码路径的两段说明与完整建议，视口仅240px高；原生文字作同宽高度基准。
/// 执行过程：渲染生产消息列表，依次缩放320/520/900/320px并读取正文、建议与按钮边界。
/// 预期结果：正文容纳全部换行及22px署名+8px间隙，建议再留24px，卡片包住按钮。
#[test]
fn staff_norma_paragraphs_reserve_height_before_proposal() {
    use norma::protocol::{StaffProposal, TaskMessage};
    let dir = tempfile::tempdir().expect("isolated project");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut task = Task::new("t", "w", "p", "布局验收");
    let text = "归档成功那条提示不该再出现。我先看现在有哪些 Agent 能接着改 pr-235。截图里那条「已归档…仍在进行，可在已归档中查看」会整条去掉，撤销一起不显示。归档本身还在。交给 Ethan 在 pr-235 上改。已提议交给 Ethan，在 `/Users/openclawd/Documents/code/norma-os/pr-235` 里改：归档后不再弹出截图里那条「已归档…仍在进行，可在已归档中查看」，右边的撤销也一起去掉。\n\n归档本身还在，任务照样进已归档。你点采用就派给他。若只想删括号里那句、撤销还要留，跟我说一声再改。".to_owned();
    task.messages
        .push(TaskMessage::Norma { text: text.clone() });
    task.proposals.push(StaffProposal {
        id: 1,
        round: 0,
        agent: "Simon".into(),
        title: "归档不显示这条提示".into(),
        why: "保留建议的完整理由".into(),
        prompt: "只修复消息与卡片的布局重叠".into(),
        model: None,
        approved: false,
        ignored: false,
        dispatch_id: None,
    });
    task.messages.push(TaskMessage::Proposal { proposal_id: 1 });
    struct Host {
        app: Entity<MultisoulApp>,
        task: Task,
        text: String,
    }
    impl gpui::Render for Host {
        fn render(
            &mut self,
            window: &mut gpui::Window,
            cx: &mut Context<Self>,
        ) -> impl gpui::IntoElement {
            use gpui::{div, prelude::*, relative};
            let list = self
                .app
                .update(cx, |app, cx| app.staff_message_list(&self.task, window, cx));
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(20.))
                        .right(px(20.))
                        .flex()
                        .justify_center()
                        .opacity(0.)
                        .child(
                            div()
                                .w_full()
                                .max_w(px(760.))
                                .text_size(px(14.))
                                .line_height(relative(1.7))
                                .debug_selector(|| "prose-reference".into())
                                .child(self.text.clone()),
                        ),
                )
                .child(list)
        }
    }
    let mut test = TestAppContext::single();
    let (_, cx) = test.add_window_view(|_, cx| {
        let app = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                paths,
            )
        });
        Host {
            app,
            task,
            text: text.replace('`', "").replace("\n\n", "\n"),
        }
    });
    for width in [320., 520., 900., 320.] {
        cx.simulate_resize(size(px(width), px(240.)));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
        let prose = cx.debug_bounds("staff-message-body-0").expect("Norma正文");
        let card = cx
            .debug_bounds("staff-proposal-card-1")
            .expect("完整建议卡");
        let adopt = cx.debug_bounds("staff-adopt-1").expect("采用按钮");
        let reference = cx
            .debug_bounds("prose-reference")
            .expect("同宽自然文字高度");
        assert!(
            (reference.size.width - prose.size.width).abs() < px(0.5),
            "基准与正文必须同宽"
        );
        assert!(
            prose.size.height + px(0.5) >= reference.size.height + px(30.),
            "正文不能超出盒子：{prose:?} / 自然文字 {reference:?}"
        );
        assert!(
            card.top() >= prose.bottom() + px(23.5),
            "建议须在正文下留24px：{prose:?} / {card:?}"
        );
        assert!(
            adopt.bottom() <= card.bottom(),
            "建议完整包住操作行：{adopt:?} / {card:?}"
        );
        // 六行各按20px，加5×12px间距与32px内边距共212px；200px取保守下限。
        assert!(
            card.size.height > px(200.),
            "完整建议不能被挤成薄片：{card:?}"
        );
    }
}
