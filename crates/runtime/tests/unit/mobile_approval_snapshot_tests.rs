use super::*;
use norma::mobile::ApprovalTarget;

/// 手机审批投影保留完整工具输入，决定必须匹配唯一调用、运行、会话和回合。
///
/// 数据构造：临时审计库、echo PTY、run-a/session-a/turn-a 的两个独立 Bash 调用。
/// 执行过程：接收两条 → 读取统一快照 → 错会话拒绝 → 选择第一条 → 重复拒绝 → 撤销第二条 → 再读取。
/// 预期结果：第一条只预约一次，另一条保留直到被撤销；旧身份或已经关闭的调用不能获得批准。
#[test]
fn mobile_approval_snapshot_binds_each_invocation_and_current_turn() {
    let directory = tempfile::tempdir().expect("临时项目");
    let store = storage::ApprovalAuditStore::new(directory.path().join("audit.sqlite"));
    let (mut runtime, node) = claude_audit_tests::fixture(store);
    let first = claude_audit_tests::request(node);
    let second = claude_audit_tests::request(node);
    assert!(
        runtime.accept_mobile_approval(first.clone()),
        "接受第一条独立审批"
    );
    assert!(
        runtime.accept_mobile_approval(second.clone()),
        "同样的工具内容也必须保留第二次调用"
    );
    let pending = runtime.mobile_approval_snapshots(node).expect("统一投影");
    assert_eq!(pending.len(), 2, "不能按工具名或正文去重");
    assert_eq!(
        pending[0].target.request_id,
        first.id(),
        "第一条绑定准确调用"
    );
    assert_eq!(
        pending[1].target.request_id,
        second.id(),
        "第二条不能被第一条替换"
    );
    assert_eq!(
        pending[0].details["command"], "printf exact",
        "必须显示真实工具输入"
    );
    let mut stale: ApprovalTarget = pending[0].target.clone();
    stale.session_id = "old-session".into();
    assert_eq!(
        runtime.choose_mobile_approval(&stale, ApprovalDecision::Allow),
        Err("approval_changed"),
        "同卡另一会话不能审批"
    );
    assert_eq!(
        first.status(Instant::now()),
        ApprovalStatus::Pending,
        "错误身份不能预约选择"
    );
    assert_eq!(
        runtime.choose_mobile_approval(&pending[0].target, ApprovalDecision::Deny),
        Ok(()),
        "当前请求可决定一次"
    );
    assert_eq!(
        runtime.choose_mobile_approval(&pending[0].target, ApprovalDecision::Allow),
        Err("approval_changed"),
        "不能覆盖已预约决定"
    );
    assert_eq!(
        second.status(Instant::now()),
        ApprovalStatus::Pending,
        "第一条决定不能影响第二条"
    );
    second.cancel();
    assert!(
        runtime
            .mobile_approval_snapshots(node)
            .expect("重新读取")
            .is_empty(),
        "已预约和已撤销调用都不再作为待决策项"
    );
    assert_eq!(
        runtime.choose_mobile_approval(&pending[1].target, ApprovalDecision::Allow),
        Err("approval_changed"),
        "桌面撤销后旧手机卡片失效"
    );
    runtime.kill_all();
}
