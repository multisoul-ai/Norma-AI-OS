//! 后台大脑正在判断的审批：在“需要你处理”里改成“Norma 判断中”，用户仍可点“定位”接管。

use super::super::overview_rows::{OverviewAction, OverviewScreen, OverviewStatusTone};
use std::collections::HashSet;

pub(super) const JUDGING_LABEL_KEY: &str = "drawers.norma_judging";

pub(super) fn relabel_judging(screen: &mut OverviewScreen, judging: &HashSet<(String, u64)>) {
    if judging.is_empty() {
        return;
    }
    let Some(attention) = screen.sections.first_mut() else {
        return;
    };
    for row in &mut attention.rows {
        let OverviewAction::LocateApproval {
            area_id, node_id, ..
        } = &row.action
        else {
            continue;
        };
        if !judging.contains(&(area_id.clone(), *node_id)) {
            continue;
        }
        let title = row
            .primary
            .split_once(" · ")
            .map_or(row.primary.as_str(), |(_, rest)| rest);
        let label = ui::i18n::text(JUDGING_LABEL_KEY);
        row.primary = format!("{label} · {title}");
        row.status_label = Some(label.into());
        row.status_tone = OverviewStatusTone::Running;
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/overview_judging_tests.rs"]
mod tests;
