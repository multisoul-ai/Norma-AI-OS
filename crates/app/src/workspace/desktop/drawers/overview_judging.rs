//! 后台大脑正在判断的审批：在“需要你处理”里改成“Norma 判断中”，用户仍可点“定位”接管。

use super::super::overview_rows::{OverviewAction, OverviewScreen, OverviewStatusTone};
use std::collections::HashSet;

pub(super) const JUDGING_LABEL: &str = "Norma 判断中";

pub(super) fn relabel_judging(screen: &mut OverviewScreen, judging_nodes: &HashSet<u64>) {
    if judging_nodes.is_empty() {
        return;
    }
    let Some(attention) = screen.sections.first_mut() else {
        return;
    };
    for row in &mut attention.rows {
        let OverviewAction::LocateApproval { node_id, .. } = &row.action else {
            continue;
        };
        if !judging_nodes.contains(node_id) {
            continue;
        }
        let title = row
            .primary
            .split_once(" · ")
            .map_or(row.primary.as_str(), |(_, rest)| rest);
        row.primary = format!("{JUDGING_LABEL} · {title}");
        row.status_label = Some(JUDGING_LABEL.into());
        row.status_tone = OverviewStatusTone::Running;
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/overview_judging_tests.rs"]
mod tests;
