use super::*;
use crate::island::{IslandAlert, IslandAlertKind, IslandPreferences};

fn alert(id: &str, kind: IslandAlertKind) -> IslandAlert {
    IslandAlert {
        id: id.into(),
        kind,
    }
}

fn hidden() -> IslandPreferences {
    IslandPreferences {
        hidden: true,
        ..Default::default()
    }
}

/// 场景：岛可见时不发系统通知；隐藏后才为待处理的审批/提问兜底，且每个请求只通知一次。
/// 数据：一个审批 a、一个提问 q。
/// 执行：先以可见偏好投递，再隐藏投递两次，随后 a 被处理只剩 q，最后 a 以同 ID 再次出现。
/// 预期：可见时为空；隐藏首次返回 a、q；重复投递为空；a 消失后再出现的同 ID 视为新请求重新通知。
#[test]
fn notifies_each_hidden_request_once_and_forgets_resolved_ones() {
    let mut gate = NotificationGate::default();
    let alerts = [
        alert("a", IslandAlertKind::Approval),
        alert("q", IslandAlertKind::Question),
    ];
    assert!(
        gate.fresh(IslandPreferences::default(), &alerts).is_empty(),
        "岛可见时由岛自身提示，不重复发系统通知"
    );
    let first: Vec<_> = gate
        .fresh(hidden(), &alerts)
        .into_iter()
        .map(|a| a.id)
        .collect();
    assert_eq!(first, ["a", "q"], "隐藏后待处理请求必须有系统入口");
    assert!(
        gate.fresh(hidden(), &alerts).is_empty(),
        "同一请求不得重复通知"
    );
    assert!(
        gate.fresh(hidden(), &alerts[1..]).is_empty(),
        "已处理的请求不再通知"
    );
    let again: Vec<_> = gate
        .fresh(hidden(), &alerts)
        .into_iter()
        .map(|a| a.id)
        .collect();
    assert_eq!(again, ["a"], "已消失的请求重新出现应视为新请求");
}

/// 场景：隐藏时仍遵守每类主动提醒开关；取消隐藏会清空已通知记录，避免重新隐藏时漏报。
/// 数据：关闭审批提醒；审批 a 与提问 q 并存。
/// 执行：隐藏投递；随后恢复可见再隐藏。
/// 预期：只通知提问；再次隐藏时 q 重新通知（可见期间已被岛展示过，但用户再次隐藏需要入口）。
#[test]
fn hidden_notifications_respect_per_kind_toggles() {
    let mut gate = NotificationGate::default();
    let prefs = IslandPreferences {
        approvals: false,
        ..hidden()
    };
    let alerts = [
        alert("a", IslandAlertKind::Approval),
        alert("q", IslandAlertKind::Question),
    ];
    let first: Vec<_> = gate
        .fresh(prefs, &alerts)
        .into_iter()
        .map(|a| a.id)
        .collect();
    assert_eq!(first, ["q"], "关闭审批弹出后审批不应通知");
    gate.fresh(IslandPreferences::default(), &alerts);
    let after: Vec<_> = gate
        .fresh(prefs, &alerts)
        .into_iter()
        .map(|a| a.id)
        .collect();
    assert_eq!(after, ["q"], "重新隐藏后仍待处理的请求要再次给出入口");
}
