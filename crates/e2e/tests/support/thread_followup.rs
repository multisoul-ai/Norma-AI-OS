use serde_json::json;
use std::{
    io::Write,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

pub fn serve(
    receiver: async_channel::Receiver<app::LiveTerminalRequest>,
    turn: Arc<Mutex<String>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        while let Ok(call) = receiver.recv_blocking() {
            let page =
                super::thread_terminal::page_for_turn(&call, &turn.lock().expect("测试前台回合"));
            let _ = call.reply.send(Ok(page));
        }
    })
}

pub fn card() -> norma::mobile::CardSnapshot {
    serde_json::from_value(
        json!({"node_id":2,"profile":"Codex","title":"Original Agent",
        "mobile_control":true,"state":"done","run_id":"review-run",
        "session_id":"review-session","turn_id":null,"task_title":null}),
    )
    .expect("原任务会话快照fixture")
}

pub fn finish(
    server: &app::MobileServer,
    receiver: async_channel::Receiver<app::LiveCommandRequest>,
    database: &Path,
    input: &mut impl Write,
    output: &mut impl std::io::BufRead,
    current_turn: &Arc<Mutex<String>>,
) {
    assert_eq!(
        super::line(output)["phase"],
        "followup-queued",
        "正式中继必须完成追问入队"
    );
    //2Hz queue polling: 500ms + bounded foreground/scheduling margin 1500ms =2s.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let call = loop {
        if let Ok(call) = receiver.try_recv() {
            break call;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "追问应进入现有前台队列"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    let app::MobileCommand::ThreadFollowup {
        target,
        expected,
        text,
    } = &call.request.command
    else {
        panic!("追问必须带原任务身份进入原消息通道，不可创建ThreadTask");
    };
    assert_eq!(
        target.task_ref, "test-runtime:7",
        "真实输入前仍必须核对完整原任务"
    );
    assert_eq!(
        target.key.turn_id, "original-turn",
        "首条追问锚定原Provider回合"
    );
    assert_eq!(expected.node_id, 2, "追问只能进入原节点");
    assert_eq!(
        expected.session_id.as_deref(),
        Some("review-session"),
        "不能替换原会话"
    );
    assert_eq!(text, "为什么这样修改首屏？", "来源正文不能丢失或改写");
    assert_eq!(
        call.request.route_id, "route-a",
        "不能使用重连时手机提交的替代路由"
    );
    assert!(call.lease.begin(), "只有原队列的第一次前台调用可执行");
    super::thread_review::seed_with_summary(
        database,
        &call.source,
        "followup-turn",
        "追问回复：首屏需要保留当前目标。",
    );
    *current_turn.lock().expect("测试回合") = "followup-turn".into();
    call.reply
        .send(Ok(
            json!({"node_id":2,"run_id":"review-run","session_id":"review-session",
        "turn_id":"followup-turn"}),
        ))
        .expect("独立Provider回合确认fixture");
    writeln!(input, "followup-sent").expect("读取后续回复");
    assert_eq!(
        super::line(output)["phase"],
        "followup-second-queued",
        "第二次追问仍关联原任务"
    );
    let second = receiver.recv_blocking().expect("连续追问必须到前台");
    let app::MobileCommand::ThreadFollowup {
        target,
        expected,
        text,
    } = &second.request.command
    else {
        panic!("第二条仍必须携带原任务身份");
    };
    assert_eq!(target.task_ref, "test-runtime:7", "连续追问不新建执行任务");
    assert_eq!(
        target.key.turn_id, "followup-turn",
        "新输入必须锚定第一条追问实际确认的回合"
    );
    assert_eq!(
        expected.session_id.as_deref(),
        Some("review-session"),
        "不得创建新session"
    );
    assert_eq!(text, "再解释一下交接状态。", "第二条正文必须独立保存");
    assert!(second.lease.begin(), "第二条只取得一次执行权");
    super::thread_review::seed_with_summary(
        database,
        &second.source,
        "followup-second-turn",
        "第二次回复：交接状态保持可见。",
    );
    *current_turn.lock().expect("测试回合") = "followup-second-turn".into();
    second.reply.send(Ok(json!({"node_id":2,"run_id":"review-run","session_id":"review-session","turn_id":"followup-second-turn"})))
        .expect("第二次Provider确认");
    writeln!(input, "followup-second-sent").expect("读取第二条回复");
    assert_eq!(
        super::line(output)["phase"],
        "followup-revoke",
        "重连必须读回本次追问结果"
    );
    assert!(
        receiver.try_recv().is_err(),
        "错目标、重试和重连不能产生第二次执行"
    );
    server
        .revoke("86AAD6D7-2A37-4D69-BAF7-03C424336A35")
        .expect("撤销测试手机");
    writeln!(input, "revoked").expect("验证撤权");
    assert_eq!(
        super::line(output)["phase"],
        "followup-finished",
        "撤权后不可恢复追问权限"
    );
}
