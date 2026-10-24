use super::*;
use crate::workspace::desktop::overview_rows::{
    overview_row_height, OverviewRow, OverviewRowKind, OverviewSection,
};

fn approval_row(node_id: u64) -> OverviewRow {
    OverviewRow {
        primary: "待审核 · 运行测试".into(),
        secondary: "Claude · 项目".into(),
        detail: None,
        task_line: None,
        project_tag: None,
        is_agent: false,
        action_label: "定位".into(),
        action: OverviewAction::LocateApproval {
            area_id: "a".into(),
            node_id,
            run_id: "r".into(),
            turn_id: "t".into(),
            request_id: Some("req".into()),
        },
        status_label: Some("待审核".into()),
        lead_label: None,
        status_tone: OverviewStatusTone::Approval,
        approval_badge: None,
        progress: None,
        prelight: Some(("a".into(), node_id)),
        enabled: true,
        height: overview_row_height(OverviewRowKind::Attention),
    }
}

fn screen(rows: Vec<OverviewRow>) -> OverviewScreen {
    OverviewScreen {
        sections: vec![OverviewSection {
            title: "需要你处理".into(),
            count: Some(rows.len()),
            rows,
        }],
    }
}

/// 大脑正在判断的审批改标为“Norma 判断中”，其他审批不变，点击动作仍是定位。
///
/// 数据构造：两条审批行（区域 a），node 3 与 node 4；大脑只在判断（a，3）。
/// 预期结果：
///   - 正断言：node 3 的标题为“Norma 判断中 · 运行测试”，状态标签同名，色调是 Running，动作仍是 LocateApproval
///   - 负断言：node 4 保持“待审核”，色调仍是 Approval
#[test]
fn only_judged_approvals_are_relabelled() {
    let mut view = screen(vec![approval_row(3), approval_row(4)]);
    relabel_judging(&mut view, &HashSet::from([("a".to_string(), 3)]));
    let judged = &view.sections[0].rows[0];
    assert_eq!(judged.primary, "Norma 判断中 · 运行测试", "标题前缀应替换");
    assert_eq!(judged.status_label.as_deref(), Some(JUDGING_LABEL));
    assert_eq!(judged.status_tone, OverviewStatusTone::Running);
    assert!(
        matches!(
            judged.action,
            OverviewAction::LocateApproval { node_id: 3, .. }
        ),
        "仍可定位接管"
    );
    let other = &view.sections[0].rows[1];
    assert_eq!(other.primary, "待审核 · 运行测试", "没被判断的审批不变");
    assert_eq!(other.status_tone, OverviewStatusTone::Approval);
}

/// 没有判断中的审批时，概览保持原样。
///
/// 预期结果：空集合不改任何行（逐字段比较）。
#[test]
fn nothing_changes_when_the_brain_is_not_judging() {
    let mut view = screen(vec![approval_row(3)]);
    let before = view.clone();
    relabel_judging(&mut view, &HashSet::new());
    assert_eq!(view, before, "空集合不应修改概览");
}
