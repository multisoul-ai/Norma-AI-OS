use super::layout_tests::MessageListHost;
use super::*;
use gpui::{px, size, AppContext, TestAppContext};

/// 场景：已派发的长提议卡，卡片背景必须包住「已派发」按钮。
/// 数据构造：900x700 窗口；含长原话的提议，已有 dispatch_id。
/// 执行过程：渲染生产消息列表一帧，读取卡片与按钮 bounds。
/// 预期结果：按钮底部不低于卡片底部。
#[test]
fn staff_dispatched_proposal_card_contains_its_button() {
    use norma::protocol::{StaffProposal, TaskMessage};
    let dir = tempfile::tempdir().expect("isolated project");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut task = Task::new("t", "w", "p", "派活");
    task.proposals.push(StaffProposal {
        id: 1,
        agent: "Ethan".into(),
        title: "派活可选类型和模型".into(),
        prompt: "用户原话：我发现幕僚指派的 Work 无法选择智能体类型。请在当前项目源码里让幕僚指派 Work 时可以同时选择智能体类型和模型，并让指派结果真正用上这两个选择。验收：派活界面能选类型和模型。待你确认（幕僚假设，不是用户原话）：只改幕僚指派这条路径，不改无关流程。".into(),
        why: "幕僚指派目前选不了".into(),
        model: None,
        approved: true,
        ignored: false,
        dispatch_id: Some("d1".into()),
    });
    task.messages.push(TaskMessage::Proposal { proposal_id: 1 });
    task.messages.push(TaskMessage::WorkerStarted {
        proposal_id: 1,
        dispatch_id: "d1".into(),
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
        MessageListHost { app, task }
    });
    for width in [420., 560., 700., 900.] {
        cx.simulate_resize(size(px(width), px(700.)));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
        let card = cx
            .debug_bounds("staff-proposal-card-1")
            .expect("proposal card");
        let button = cx.debug_bounds("staff-adopt-1").expect("adopt button");
        assert!(
            button.bottom() <= card.bottom() + px(0.5),
            "宽度 {width} 按钮溢出卡片：{button:?} / {card:?}"
        );
    }
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
