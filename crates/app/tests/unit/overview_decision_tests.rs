use super::*;
use crate::workspace::desktop::overview_rows::{
    overview_row_height, OverviewAction, OverviewRowKind, OverviewStatusTone,
};

fn row(action: OverviewAction, enabled: bool) -> OverviewRow {
    OverviewRow {
        primary: "待审核 · 运行测试".into(),
        secondary: "Claude · 项目".into(),
        detail: None,
        task_line: None,
        project_tag: None,
        is_agent: false,
        action_label: "定位".into(),
        action,
        status_label: Some("待审核".into()),
        lead_label: None,
        status_tone: OverviewStatusTone::Approval,
        approval_badge: None,
        progress: None,
        prelight: None,
        enabled,
        height: overview_row_height(OverviewRowKind::Attention),
    }
}

fn locate(request_id: Option<&str>) -> OverviewAction {
    OverviewAction::LocateApproval {
        area_id: "area-1".into(),
        node_id: 7,
        run_id: "run-1".into(),
        turn_id: "turn-1".into(),
        request_id: request_id.map(str::to_string),
    }
}

/// 只有带请求标识、且行可用的审批行才提供“允许 / 拒绝”的目标。
///
/// 数据构造：有标识且可用；没有标识（只是终端里的观察状态）；有标识但行被禁用；非审批动作。
/// 预期结果：
///   - 正断言：第一种返回（area-1，7，run-1，req-1）
///   - 负断言：其余三种都没有目标，不会给无法处理的审批画出按钮
#[test]
fn only_approvable_rows_offer_allow_and_deny() {
    assert_eq!(
        decision_target(&row(locate(Some("req-1")), true)),
        Some(("area-1".into(), 7, "run-1".into(), "req-1".into())),
        "有请求标识且可用时应给出目标"
    );
    assert_eq!(
        decision_target(&row(locate(None), true)),
        None,
        "没有请求标识不能处理"
    );
    assert_eq!(
        decision_target(&row(locate(Some("req-1")), false)),
        None,
        "禁用的行不提供按钮"
    );
    let other = OverviewAction::LocateAgent {
        area_id: "a".into(),
        node_id: 1,
    };
    assert_eq!(decision_target(&row(other, true)), None, "非审批行没有按钮");
}
