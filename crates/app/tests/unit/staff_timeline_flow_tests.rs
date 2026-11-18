use super::*;
use gpui::{div, prelude::*, px, relative, size, AppContext, TestAppContext};
use norma::protocol::{
    ReplyStatus, ReplyTrigger, StepStatus, StepTitle, TaskMessage, Worker, WorkerOutcome,
    WorkerPhase,
};

struct FlowHost {
    app: Entity<MultisoulApp>,
    task: Task,
    reply: String,
    result: String,
}

impl gpui::Render for FlowHost {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let list = self
            .app
            .update(cx, |app, cx| app.staff_message_list(&self.task, window, cx));
        // 同宽纯文本基准，不进滚动流。幕僚正文 14×1.7 满列宽；结果正文在卡片左右各 16px 内，13px / 20px 行高。
        div().size_full().child(list).child(
            div()
                .absolute()
                .left(px(20.))
                .right(px(20.))
                .top(px(0.))
                .opacity(0.)
                .flex()
                .flex_col()
                .items_center()
                .child(
                    div()
                        .w_full()
                        .max_w(px(760.))
                        .text_size(px(14.))
                        .line_height(relative(1.7))
                        .debug_selector(|| "timeline-reply-reference".into())
                        .child(self.reply.clone()),
                )
                .child(
                    div()
                        .w_full()
                        .max_w(px(760.))
                        .px(px(16.))
                        .text_size(px(13.))
                        .line_height(px(20.))
                        .debug_selector(|| "timeline-result-reference".into())
                        .child(self.result.clone()),
                ),
        )
    }
}

fn done_reply(text: &str, steps: usize, elapsed_ms: u64) -> TaskMessage {
    let mut reply = norma::protocol::StaffReply::new(0, ReplyTrigger::User);
    for index in 0..steps {
        reply.upsert_step(
            &format!("step-{index}"),
            Some(StepTitle::Tool(format!("read-{index}"))),
            Some(StepStatus::Completed),
            Some(1_000),
        );
    }
    reply.push_text(text);
    reply.finish(ReplyStatus::Done, elapsed_ms);
    TaskMessage::Staff(reply)
}

/// 场景：幕僚长回复被下一条用户气泡盖住；Worker 结果卡底部被下一条幕僚（头像、步骤条、正文）盖住。
///
/// 数据构造：
///   窗口高 480，宽依次 320 / 520 / 900，内容高于视口
///   回复 A：一段 180 字中文，无步骤。列宽 = min(窗口宽 − 左右垫 40, 760)
///   用户气泡：短句，接在回复 A 后面
///   结果卡：Bella，正文 220 字且以「触发链」收尾。卡片左右内边距 16，所以正文宽 = 列宽 − 32
///   回复 B：4 个已完成步骤，耗时 79500ms →「已处理 4 步 · 用时 79.5 秒」，再加一段正文
///   另有 4 条短消息交替，用来看窄列里连续消息会不会叠
///
/// 执行过程：
///   1. 渲染生产消息列表
///   2. 同宽纯文本作为高度下限（Markdown 单段不应矮于同样的换行）
///   3. 读取回复、气泡、结果正文、下一条幕僚、步骤条的边界
///
/// 预期结果：
///   - 回复 A 的布局高度不低于同宽纯文本，用户气泡顶不高于回复正文底
///   - 结果正文高度不低于同宽纯文本，下一条幕僚顶不高于结果正文底
///   - 步骤条落在自己那条消息里，复制操作还在
///   - 相邻消息盒子不相交，间距约 24px
#[test]
fn staff_timeline_items_keep_their_own_height() {
    let dir = tempfile::tempdir().expect("isolated project");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let reply = "卡片上如果已经写完就以卡片为准。".repeat(10);
    let result = format!("{}触发链", "结果里这一段必须完整留在卡片里。".repeat(8));
    let mut task = Task::new("t", "w", "p", "重叠");
    let mut worker = Worker::new("Bella", "claude", "", 7, "d1");
    worker.phase = WorkerPhase::Finished(WorkerOutcome::Succeeded);
    task.workers.push(worker);
    task.messages.push(done_reply(&reply, 0, 1_000));
    task.messages.push(TaskMessage::User {
        text: "为什么要加这个限制".into(),
    });
    task.messages.push(TaskMessage::WorkerFinished {
        node_id: 7,
        dispatch_id: "d1".into(),
        outcome: WorkerOutcome::Succeeded,
        text: result.clone(),
    });
    task.messages
        .push(done_reply("下一条幕僚不该画进结果卡。", 4, 79_500));
    for index in 0..2 {
        task.messages.push(TaskMessage::User {
            text: format!("短消息 {index}"),
        });
        task.messages
            .push(done_reply(&format!("短回复 {index}"), 0, 200));
    }
    let reply_for_host = reply.clone();
    let result_for_host = result.clone();
    let mut test = TestAppContext::single();
    let (_, cx) = test.add_window_view(|_, cx| {
        let app = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                paths,
            )
        });
        FlowHost {
            app,
            task,
            reply: reply_for_host,
            result: result_for_host,
        }
    });
    for width in [320., 520., 900.] {
        cx.simulate_resize(size(px(width), px(480.)));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
        let reply_text = cx.debug_bounds("staff-reply-text-0-0").expect("reply text");
        let reply_ref = cx
            .debug_bounds("timeline-reply-reference")
            .expect("reply reference");
        let bubble = cx.debug_bounds("staff-user-bubble-1").expect("user bubble");
        let result_body = cx.debug_bounds("staff-result-body-2").expect("result body");
        let result_ref = cx
            .debug_bounds("timeline-result-reference")
            .expect("result reference");
        let next = cx.debug_bounds("staff-message-3").expect("next staff");
        let process = cx.debug_bounds("staff-process-3").expect("step toggle");
        let actions = cx
            .debug_bounds("staff-reply-actions-0")
            .expect("reply actions");
        assert!(
            reply_text.size.height + px(2.) >= reply_ref.size.height,
            "宽 {width}：幕僚正文布局矮于同宽文字，下一条会盖住溢出的行：正文 {reply_text:?} / 基准 {reply_ref:?}"
        );
        assert!(
            bubble.top() + px(0.5) >= reply_text.bottom(),
            "宽 {width}：用户气泡盖住上一条幕僚正文：气泡 {bubble:?} / 正文 {reply_text:?}"
        );
        assert!(
            result_body.size.height + px(2.) >= result_ref.size.height,
            "宽 {width}：结果卡正文布局矮于同宽文字：正文 {result_body:?} / 基准 {result_ref:?}"
        );
        assert!(
            next.top() + px(0.5) >= result_body.bottom(),
            "宽 {width}：下一条幕僚画进结果卡：下一条 {next:?} / 结果 {result_body:?}"
        );
        assert!(
            process.top() + px(0.5) >= next.top() && process.bottom() <= next.bottom() + px(0.5),
            "宽 {width}：步骤条必须落在自己的消息里：步骤 {process:?} / 消息 {next:?}"
        );
        assert!(
            actions.top() + px(0.5) >= reply_text.bottom() && actions.size.height >= px(28.),
            "复制操作还在，且在正文下面：{actions:?} / {reply_text:?}"
        );
        let mut previous = cx.debug_bounds("staff-message-0").expect("first");
        let selectors = [
            "staff-message-1",
            "staff-message-2",
            "staff-message-3",
            "staff-message-4",
            "staff-message-5",
            "staff-message-6",
            "staff-message-7",
        ];
        for (index, selector) in selectors.iter().enumerate() {
            let index = index + 1;
            let current = cx
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("message {index}"));
            assert!(
                current.top() + px(0.5) >= previous.bottom(),
                "宽 {width}：消息 {index} 与上一条重叠：{current:?} / {previous:?}"
            );
            let gap = current.top() - previous.bottom();
            assert!(
                gap <= px(28.) && gap >= px(20.),
                "宽 {width}：消息 {index} 间距应是列表的 24px，实际 {gap:?}"
            );
            previous = current;
        }
    }
}
