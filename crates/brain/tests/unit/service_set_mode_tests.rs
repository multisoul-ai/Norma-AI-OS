use super::*;

/// 场景：适配器升级后不认我们请求的模式 id（Codex ACP 2.2.2 对 `auto` 返回 -32602），
/// 幕僚会话仍应建立并正常工作，而不是整条握手失败、一直报“大脑后端不可用”。
///
/// 数据构造：假适配器开启 FAKE_REJECT_SET_MODE，session/set_mode 一律回 -32602 Invalid params；
/// 提交审批 a1（cargo test），审批会话照提示执行 brain decide。
///
/// 执行过程：
///   1. initialize、session/new 成功，拿到 sessionId
///   2. session/set_mode default → -32602，只记警告，继续
///   3. 会话可用：审批提示送达，decide 命令经权限请求被放行，本轮结束
///
/// 预期结果：
///   - 正断言：日志里出现过 `set_mode default`（确实发过且被拒）
///   - 正断言：日志里出现 decide 命令的 `allowed:`，并收到 ApprovalTurnEnded(a1)
///   - 负断言：没有 BackendDown（握手没有因 set_mode 失败而中断）
#[test]
fn rejected_set_mode_does_not_abort_the_session() {
    let h = start_with_env(vec![("FAKE_REJECT_SET_MODE".into(), "1".into())]);
    h.push(approval("a1", "cargo test"));
    let down = std::cell::Cell::new(None);
    h.wait_status(|s| {
        if let BrainStatusKind::BackendDown { reason } = s {
            down.set(Some(reason.clone()));
            return true;
        }
        matches!(s, BrainStatusKind::ApprovalTurnEnded { id } if id == "a1")
    });
    let log = h.log();
    assert_eq!(
        down.take(),
        None,
        "set_mode 被拒不应让后端被判为不可用：\n{log}"
    );
    assert!(
        log.contains("set_mode default"),
        "应发送过 set_mode 且被拒：\n{log}"
    );
    assert!(
        log.contains(&format!("allowed: {NORMA} brain decide a1 allow")),
        "会话仍应可用，decide 命令应被放行：\n{log}"
    );
    h.service.shutdown();
}
