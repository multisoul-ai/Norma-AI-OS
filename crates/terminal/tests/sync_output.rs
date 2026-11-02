use crossbeam_channel::unbounded;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use terminal::{PtyEvent, TerminalSession, TestPtyBackend};

/// 场景：终端开启同步输出后等待DSR回复，却不发送结束同步标记；不能永久互相等待。
/// 构造：BSU + ABC + CSI6n，真实vte0.15同步预算150ms，记录后端回写；ABC后的坐标为1行4列。
/// 执行：先确认同步期间不发布ABC、不提前回复；200ms后只调用App同样的drain，不再投递字节。
/// 预期：idle drain主动到期同步批次，发布ABC并精确回写一次DSR；后续drain不能重复发布。
#[test]
fn idle_drain_expires_synchronized_output_and_replies_without_another_pty_chunk() {
    let (tx, rx) = unbounded();
    let writes = Arc::new(Mutex::new(Vec::new()));
    let backend = TestPtyBackend::recording(tx.clone(), writes.clone());
    let mut session = TerminalSession::spawn_with_backend(1, 80, 24, Box::new(backend), rx);
    tx.send(PtyEvent::Output(b"\x1b[?2026hABC\x1b[6n".to_vec()))
        .unwrap();
    session.drain();
    assert_eq!(
        session.snapshot().rows[0][0].c,
        ' ',
        "同步批次到期前不可拆开显示"
    );
    assert!(
        writes.lock().unwrap().is_empty(),
        "未处理的同步DSR不能伪报已回复"
    );
    std::thread::sleep(Duration::from_millis(200));
    assert!(session.drain(), "没有新PTY字节时也必须服务150ms同步到期");
    let text: String = session.snapshot().rows[0]
        .iter()
        .map(|cell| cell.c)
        .collect();
    assert!(text.starts_with("ABC"), "超时必须发布完整原批次：{text:?}");
    assert_eq!(
        *writes.lock().unwrap(),
        vec![b"\x1b[1;4R".to_vec()],
        "ABC后DSR必须精确回复1行4列且只写一次"
    );
    assert!(!session.drain(), "已到期批次不能使后续idle drain持续dirty");
    assert_eq!(writes.lock().unwrap().len(), 1, "不能重复写回已处理的DSR");
}

/// 场景：正常程序及时结束同步批次；构造BSU/ABC和ESU两个独立chunk。
/// 执行：结束前检查画面仍空，投递ESU后立刻drain；预期立即发布，不等待超时兜底。
#[test]
fn explicit_sync_end_publishes_immediately_and_does_not_replay() {
    let (tx, rx) = unbounded();
    let writes = Arc::new(Mutex::new(Vec::new()));
    let backend = TestPtyBackend::recording(tx.clone(), writes);
    let mut session = TerminalSession::spawn_with_backend(2, 80, 24, Box::new(backend), rx);
    tx.send(PtyEvent::Output(b"\x1b[?2026hABC".to_vec()))
        .unwrap();
    session.drain();
    assert_eq!(
        session.snapshot().rows[0][0].c,
        ' ',
        "正常同步期间应合并画面"
    );
    tx.send(PtyEvent::Output(b"\x1b[?2026l".to_vec())).unwrap();
    assert!(session.drain(), "显式结束同步必须立即变为dirty");
    assert_eq!(
        session.snapshot().rows[0][0].c,
        'A',
        "显式结束不能继续隐藏原输出"
    );
    assert!(!session.drain(), "正常结束后不能重复回放批次");
}

/// 场景：子进程在同步批次中退出，不能把最后正文藏到未来的150ms定时器。
/// 构造：BSU+FINAL紧接真实形状的Exited(7)；一次drain处理两条事件。
/// 预期：立即显示全部正文，退出码仍为7；退出本身不应替换或丢弃缓冲内容。
#[test]
fn process_exit_publishes_the_last_synchronized_output_immediately() {
    let (tx, rx) = unbounded();
    let backend = TestPtyBackend::recording(tx.clone(), Arc::new(Mutex::new(Vec::new())));
    let mut session = TerminalSession::spawn_with_backend(3, 80, 24, Box::new(backend), rx);
    tx.send(PtyEvent::Output(b"\x1b[?2026hFINAL".to_vec()))
        .unwrap();
    tx.send(PtyEvent::Exited { code: Some(7) }).unwrap();
    assert!(session.drain(), "退出及最后输出必须触发更新");
    let text: String = session.snapshot().rows[0]
        .iter()
        .map(|cell| cell.c)
        .collect();
    assert!(
        text.starts_with("FINAL"),
        "进程退出不能留下未发布的最后批次：{text:?}"
    );
    assert_eq!(
        session.status,
        terminal::TerminalStatus::Exited { code: Some(7) },
        "同步输出收口必须保留原退出码"
    );
}
