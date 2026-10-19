use super::{approval_is_actionable, approval_is_pending, approval_request_matches};

/// 场景：概览只有实时且带完整请求身份的待审项才可直达。
/// 数据构造：一条待审核状态，分别提供完整、缺失与缓存来源的身份。
/// 执行过程：查询概览和只读快照共用的可操作判断。
/// 预期结果：只有实时、run 和 turn 均存在时可操作，旧状态仍可显示但不能点击审批。
#[test]
fn approval_requires_live_status_and_complete_identity() {
    assert!(
        approval_is_pending("waiting_approval"),
        "缓存与缺失请求身份都不能抹掉真实待审状态"
    );
    assert!(!approval_is_pending("ready"), "待命状态不得计入待审项");
    assert!(
        approval_is_actionable(true, "waiting_approval", Some("run-1"), Some("turn-1")),
        "实时完整待审请求应可定位"
    );
    assert!(
        !approval_is_actionable(false, "waiting_approval", Some("run-1"), Some("turn-1")),
        "缓存状态不能声称审批入口可用"
    );
    assert!(
        !approval_is_actionable(true, "waiting_approval", None, None),
        "缺少 run 和 turn 时不得开放审批入口"
    );
    assert!(
        !approval_is_actionable(true, "waiting_approval", Some("run-1"), None),
        "只拿到 run 不能当作完整请求"
    );
    assert!(
        !approval_is_actionable(true, "ready", Some("run-1"), Some("turn-1")),
        "旧请求身份不能令待命 Agent 显示为待审核"
    );
}

/// 场景：点击审批提示前必须重新核验原请求，不能用空身份匹配空快照。
/// 数据构造：当前待审卡片和 run/turn 身份，依次尝试完整、缺失和过期身份。
/// 执行过程：比较实际卡片与点击时携带的请求身份。
/// 预期结果：只接收完全相同的一组非空身份，旧回合和空值都被拒绝。
#[test]
fn approval_recheck_rejects_missing_and_stale_request_identity() {
    assert!(
        approval_request_matches(
            "waiting_approval",
            Some("run-1"),
            Some("turn-1"),
            Some("run-1"),
            Some("turn-1"),
        ),
        "同一实时请求应允许定位"
    );
    assert!(
        !approval_request_matches("waiting_approval", None, None, None, None),
        "空请求身份不得匹配空快照"
    );
    assert!(
        !approval_request_matches(
            "waiting_approval",
            Some("run-1"),
            Some("turn-1"),
            Some("run-1"),
            Some("old-turn"),
        ),
        "过期回合必须拒绝"
    );
    assert!(
        !approval_request_matches(
            "ready",
            Some("run-1"),
            Some("turn-1"),
            Some("run-1"),
            Some("turn-1"),
        ),
        "审批已结束时不得定位到旧提示"
    );
}
