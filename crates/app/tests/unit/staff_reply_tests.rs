use super::layout_tests::MessageListHost;
use super::*;
use gpui::{px, size, AppContext, TestAppContext};
use norma::protocol::{ReplyStatus, ReplyTrigger, StepStatus, StepTitle, TaskMessage};

/// 一条带两个步骤和正文的回复。
fn reply(trigger: ReplyTrigger, status: ReplyStatus) -> norma::protocol::StaffReply {
    let mut reply = norma::protocol::StaffReply::new(0, trigger);
    reply.upsert_step(
        "a",
        Some(StepTitle::Norma(vec!["brain".into(), "agents".into()])),
        Some(StepStatus::Completed),
        Some(800),
    );
    reply.upsert_step(
        "b",
        Some(StepTitle::Tool("skill".into())),
        Some(StepStatus::Running),
        None,
    );
    reply.push_text("上一轮派发没送到。");
    if status != ReplyStatus::Running {
        reply.finish(status, 53_700);
    }
    reply
}

/// 在测试窗口里渲染生产消息列表；返回 App 实体以便调整展开状态。
macro_rules! window {
    ($test:ident, $task:expr) => {{
        let dir = tempfile::tempdir().expect("isolated project");
        let paths = storage::DatabasePaths::isolated_test(dir.path());
        let mut handle = None;
        let (_, cx) = $test.add_window_view(|_, cx| {
            let app = cx.new(|cx| {
                MultisoulApp::new_with_pty_factory_and_database_paths(
                    cx,
                    Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                    paths,
                )
            });
            handle = Some(app.clone());
            MessageListHost { app, task: $task }
        });
        cx.simulate_resize(size(px(900.), px(700.)));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
        (handle.expect("app entity"), cx)
    }};
}

/// 场景：完成的回复折叠过程区，展开后才显示步骤；操作行（复制/重新生成）出现在最后一条用户触发的回复上。
/// 数据构造：[用户, 已完成回复（两步 + 正文，用户触发）]；窗口 900×700。
/// 执行过程：先渲染折叠态；再把 process-t-1 放进 expanded 重绘。
/// 预期结果：折叠时有过程区标题、复制和重新生成，没有步骤行；展开后出现步骤 a、b。
#[test]
fn finished_reply_collapses_its_process_and_offers_actions() {
    let mut task = Task::new("t", "w", "p", "派活");
    task.user_message("没派发成功");
    task.messages.push(TaskMessage::Staff(reply(
        ReplyTrigger::User,
        ReplyStatus::Done,
    )));
    let mut test = TestAppContext::single();
    let (app, cx) = window!(test, task);
    assert!(
        cx.debug_bounds("staff-process-1").is_some(),
        "过程区标题可见"
    );
    assert!(
        cx.debug_bounds("staff-step-a").is_none(),
        "完成后默认折叠，不显示步骤行"
    );
    assert!(cx.debug_bounds("staff-copy").is_some(), "完成的回复有复制");
    assert!(
        cx.debug_bounds("staff-regenerate").is_some(),
        "最后一条用户触发的回复可以重新生成"
    );
    app.update(cx, |app, _| {
        app.staff.expanded.insert("process-t-1".into());
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("staff-step-a").is_some(),
        "展开后显示步骤 a"
    );
    assert!(
        cx.debug_bounds("staff-step-b").is_some(),
        "展开后显示步骤 b"
    );
}

/// 场景：运行中的回复过程区始终展开，没有复制和重新生成；事件触发的最后一条回复也不能重新生成。
/// 数据构造：A = [用户, Running 回复]；B = [用户, Done 回复（事件触发）]。
/// 预期结果：A 无需展开就有步骤行、无操作行；B 有复制、没有重新生成。
#[test]
fn running_reply_stays_expanded_and_event_replies_cannot_regenerate() {
    let mut running = Task::new("t", "w", "p", "派活");
    running.user_message("问");
    running.messages.push(TaskMessage::Staff(reply(
        ReplyTrigger::User,
        ReplyStatus::Running,
    )));
    let mut test = TestAppContext::single();
    let (_app, cx) = window!(test, running);
    assert!(
        cx.debug_bounds("staff-step-a").is_some(),
        "运行中过程区自动展开"
    );
    assert!(cx.debug_bounds("staff-copy").is_none(), "运行中不显示复制");
    assert!(
        cx.debug_bounds("staff-regenerate").is_none(),
        "运行中不显示重新生成"
    );

    let mut event = Task::new("t2", "w", "p", "事件");
    event.user_message("问");
    event.messages.push(TaskMessage::Staff(reply(
        ReplyTrigger::Event,
        ReplyStatus::Done,
    )));
    let mut test = TestAppContext::single();
    let (_app, cx) = window!(test, event);
    assert!(cx.debug_bounds("staff-copy").is_some(), "事件回复也能复制");
    assert!(
        cx.debug_bounds("staff-regenerate").is_none(),
        "没有用户输入的回复不能重新生成"
    );
}

/// 场景：Worker 的长结果默认只显示前 3 行摘要，展开后显示全文；短结果没有展开按钮。
/// 数据构造：一条 5 行的成功结果和一条 1 行的成功结果；窗口 900×700。
/// 执行过程：先渲染折叠态，再把该结果的展开键放进 expanded 重绘。
/// 预期结果：长结果有展开按钮、第 5 行未出现在布局里之前按钮存在；短结果没有按钮；展开后按钮仍在（变为收起）。
#[test]
fn long_worker_results_collapse_to_three_lines() {
    use norma::protocol::WorkerOutcome;
    let finished = |node: u64, text: &str| TaskMessage::WorkerFinished {
        node_id: node,
        dispatch_id: format!("d{node}"),
        outcome: WorkerOutcome::Succeeded,
        text: text.into(),
    };
    let mut task = Task::new("t", "w", "p", "结果");
    task.messages.push(finished(1, "一\n二\n三\n四\n五"));
    task.messages.push(finished(2, "只有一行"));
    let mut test = TestAppContext::single();
    let (app, cx) = window!(test, task);
    assert!(
        cx.debug_bounds("staff-result-toggle-0").is_some(),
        "超过 3 行的结果有展开按钮"
    );
    assert!(
        cx.debug_bounds("staff-result-toggle-1").is_none(),
        "不超过 3 行的结果没有展开按钮"
    );
    let collapsed = cx
        .debug_bounds("staff-message-body-0")
        .expect("body")
        .size
        .height;
    app.update(cx, |app, _| {
        app.staff.expanded.insert("worker-result-t-0".into());
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let expanded = cx
        .debug_bounds("staff-message-body-0")
        .expect("body")
        .size
        .height;
    assert!(
        expanded > collapsed,
        "展开后内容更高：折叠 {collapsed:?}，展开 {expanded:?}（多出第 4、5 行）"
    );
}

/// 场景：Worker 的结果是一整段很长的话（没有换行），折行后超过 3 行。
/// 数据构造：一条 800 多字的单行结果；窗口 900×700。
/// 预期结果：有展开按钮；折叠时正文比展开时矮，不会顶到按钮上。
#[test]
fn one_long_paragraph_is_clamped_and_expandable() {
    let text = "已创建文件，单文件即可在浏览器里打开运行。".repeat(40);
    let mut task = Task::new("t", "w", "p", "结果");
    task.messages.push(TaskMessage::WorkerFinished {
        node_id: 1,
        dispatch_id: "d".into(),
        outcome: norma::protocol::WorkerOutcome::Succeeded,
        text,
    });
    let mut test = TestAppContext::single();
    let (app, cx) = window!(test, task);
    assert!(
        cx.debug_bounds("staff-result-toggle-0").is_some(),
        "长段落有展开按钮"
    );
    let collapsed = cx
        .debug_bounds("staff-message-body-0")
        .expect("body")
        .size
        .height;
    app.update(cx, |app, _| {
        app.staff.expanded.insert("worker-result-t-0".into());
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let expanded = cx
        .debug_bounds("staff-message-body-0")
        .expect("body")
        .size
        .height;
    assert!(
        expanded > collapsed,
        "展开后更高：折叠 {collapsed:?}，展开 {expanded:?}"
    );
}
