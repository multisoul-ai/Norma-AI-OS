use super::*;
fn frame(signature: u64, sequence: u64) -> NativeTerminalFrame {
    NativeTerminalFrame {
        signature,
        input_sequence: sequence,
        rows: vec!["live".into()],
    }
}

/// 场景：原生问答每次只发一键，须等待完整画面稳定；普通输入可立即抢占。
/// 构造：同一frame的重绘、变化frame、输入序号递增和5秒无回显超时。
/// 执行：观察稳定帧，记录一次写入，再观察重复画面、真实变化和人工输入。
/// 预期：未稳定和重复画面不放行；新画面稳定后可继续；人工输入或超时永远拒绝旧租约。
#[test]
fn terminal_step_lease_requires_stable_new_frame_and_original_input_sequence() {
    let now = std::time::Instant::now();
    let first = frame(1, 7);
    let mut lease = TerminalStepLease::new(&first, now);
    assert!(
        !lease
            .ready(&first, now + Duration::from_millis(499))
            .unwrap(),
        "完整控件稳定至少500ms"
    );
    assert!(
        lease
            .ready(&first, now + Duration::from_millis(500))
            .unwrap(),
        "稳定控件可以执行首键"
    );
    lease.written(&first, 8, now + Duration::from_millis(500));
    assert!(
        !lease
            .ready(&frame(1, 8), now + Duration::from_millis(1200))
            .unwrap(),
        "未收到画面变化不能重发首键"
    );
    assert!(
        !lease
            .ready(&frame(2, 8), now + Duration::from_millis(1300))
            .unwrap(),
        "新帧也需要稳定"
    );
    assert!(
        lease
            .ready(&frame(2, 8), now + Duration::from_millis(1800))
            .unwrap(),
        "变化且稳定后下一步"
    );
    assert_eq!(
        lease.ready(&frame(2, 9), now + Duration::from_millis(1900)),
        Err("question_native_takeover"),
        "人工按键使输入序号不符"
    );
    assert!(
        lease
            .ready(&frame(2, 8), now + Duration::from_millis(2000))
            .is_err(),
        "人工抢占永久撤销旧租约"
    );
    let mut lease = TerminalStepLease::new(&first, now);
    lease.written(&first, 8, now);
    assert_eq!(
        lease.ready(&frame(1, 8), now + Duration::from_secs(5)),
        Err("question_terminal_no_progress"),
        "没有原控件回显不能猜测或重放"
    );
}
