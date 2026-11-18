use super::layout_tests::MessageListHost;
use super::*;
use gpui::{px, size, AppContext, Bounds, Pixels, TestAppContext};
use norma::protocol::{ReplyStatus, ReplyTrigger, TaskMessage, Worker, WorkerOutcome, WorkerPhase};

fn reply(text: &str) -> TaskMessage {
    let mut reply = norma::protocol::StaffReply::new(0, ReplyTrigger::User);
    reply.push_text(text);
    reply.finish(ReplyStatus::Done, 1_000);
    TaskMessage::Staff(reply)
}

/// 场景：宽窗里阅读列被 760 上限夹住时，长消息的最后几行压到下一条（截图里的署名、结果卡、未完成提示）。
///
/// 数据构造：
///   窗口宽 1600 / 1280 / 900：列表左右各 20 内边距，可用宽 1560 / 1240 / 860，都大于阅读宽 760
///   窗口宽 520：可用 480 < 760，上限不起作用，作对照
///   消息 0：幕僚长回复，一段 30 句中文，约 900 字
///   消息 1：用户气泡
///   消息 2：Bella 结果卡，段落 + 10 条路径列表
///   消息 3：幕僚短回复
///
/// 执行过程：
///   1. 渲染生产消息列表一帧
///   2. 读取每条消息外框，以及它最后一块内容（回复操作行 / 气泡 / 结果正文）
///
/// 预期结果：
///   - 每条消息的最后一块内容底边不超过自己外框的底边（盒子按 760 换行后的高度量出）
///   - 下一条消息顶边不高于上一条最后一块内容的底边（不会画进上一条）
#[test]
fn staff_wide_reading_column_keeps_every_line_inside_its_message() {
    let dir = tempfile::tempdir().expect("isolated project");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let long = "卡片上如果已经写完就以卡片为准，若也断了就重新派发一次。".repeat(30);
    let files: String = (1..=10)
        .map(|i| format!("- crates/brain/tests/unit/service_lifecycle_{i}_tests.rs\n"))
        .collect();
    let result = format!(
        "{}\n\n改过的文件：\n\n{files}",
        "正常排查不再因为到点被 abort。".repeat(12)
    );
    let mut task = Task::new("t", "w", "p", "宽列");
    let mut worker = Worker::new("Bella", "claude", "", 7, "d1");
    worker.phase = WorkerPhase::Finished(WorkerOutcome::Succeeded);
    task.workers.push(worker);
    task.messages.push(reply(&long));
    task.messages.push(TaskMessage::User {
        text: "为什么要加这个限制".into(),
    });
    task.messages.push(TaskMessage::WorkerFinished {
        node_id: 7,
        dispatch_id: "d1".into(),
        outcome: WorkerOutcome::Succeeded,
        text: result,
    });
    task.messages.push(reply("两边都做完了。"));
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
    let outer = [
        "staff-message-0",
        "staff-message-1",
        "staff-message-2",
        "staff-message-3",
    ];
    // 每条消息里排在最后的那块：回复的操作行、用户气泡、结果正文、回复的操作行。
    let last = [
        "staff-reply-actions-0",
        "staff-user-bubble-1",
        "staff-result-body-2",
        "staff-reply-actions-3",
    ];
    for width in [1600., 1280., 900., 520.] {
        cx.simulate_resize(size(px(width), px(800.)));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
        let mut previous: Option<Bounds<Pixels>> = None;
        for index in 0..outer.len() {
            let message = cx.debug_bounds(outer[index]).expect("message bounds");
            let content = cx.debug_bounds(last[index]).expect("last block bounds");
            assert!(
                content.bottom() <= message.bottom() + px(0.5),
                "宽 {width}：消息 {index} 的最后一块画出了自己的外框，下一条会盖住它：内容 {content:?} / 外框 {message:?}"
            );
            if let Some(above) = previous {
                assert!(
                    message.top() + px(0.5) >= above.bottom(),
                    "宽 {width}：消息 {index} 画进了上一条：本条 {message:?} / 上一条最后一块 {above:?}"
                );
            }
            previous = Some(content);
        }
    }
}
