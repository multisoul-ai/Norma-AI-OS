use super::*;
use crate::nodes::term::TermLive;
use terminal::PtyCommand;

/// 场景：原PTY已经可写但宿主尚在首次交付窗口，普通任务不能抢占未知原生交互。
/// 构造：echo PTY可立即接收输入；one-shot宿主mailbox尚未交付，模拟后台install与App drain之间。
/// 执行：先提交普通任务，随后关闭准备通道并drain，再尝试同样的输入。
/// 预期：交付前输入被拒绝且PTY无文字；准备已结束后不遗留永久输入锁。
#[test]
fn task_input_waits_for_native_host_handover() {
    let mut runtime = Runtime::new();
    let factory: crate::PtyFactory =
        Box::new(|_, _, _, sender| Ok(Box::new(terminal::TestPtyBackend::echo(sender))));
    runtime.set_pty_factory(Box::new(|_, _, _, sender| {
        Ok(Box::new(terminal::TestPtyBackend::echo(sender)))
    }));
    let node = runtime
        .spawn_agent(
            canvas::AgentProfile::Codex,
            canvas::CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .unwrap();
    let (sender, receiver) = crossbeam_channel::bounded(1);
    let terminal = TermLive::spawn_with_factory(999, PtyCommand::new("unused"), 80, 24, &factory)
        .unwrap()
        .with_native_question_host(receiver);
    runtime.live.insert(node, Box::new(terminal));
    let held = runtime.agent_task_input_held(node);
    let early = runtime.send_agent_task_text(node, "must not reach question");
    runtime.drain_all();
    let Some(crate::NodeView::Term { snapshot, .. }) = runtime.node_view(node) else {
        panic!("测试PTY必须存在")
    };
    let screen: String = snapshot.rows.iter().flatten().map(|cell| cell.c).collect();
    drop(sender);
    runtime.drain_native_question_hosts("p");
    assert!(held, "首次宿主交付前普通任务必须等待");
    assert!(early.is_err(), "可写PTY不代表问答宿主已经接管");
    assert!(!screen.contains("must not reach"), "不能先写入再报告失败");
    assert!(
        !runtime.agent_task_input_held(node),
        "准备通道关闭后不遗留交付锁"
    );
    assert!(
        runtime.send_agent_task_text(node, "ready").is_ok(),
        "交付结束后普通PTY仍可输入"
    );
}
