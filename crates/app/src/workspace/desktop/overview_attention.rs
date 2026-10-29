//! Presentation of attention rows; user titles stay unchanged.
use super::{
    overview_row_height, OverviewAction, OverviewRow, OverviewRowKind, OverviewStatusTone,
};
use norma::mobile::{RunOverviewAttention, RunOverviewAttentionKind};

pub(super) fn attention_row(row: &RunOverviewAttention) -> OverviewRow {
    let (prefix, tone) = match row.kind {
        RunOverviewAttentionKind::Failed => {
            (ui::i18n::text("drawers.error"), OverviewStatusTone::Failed)
        }
        RunOverviewAttentionKind::Approval => (
            ui::i18n::text("drawers.pending_approval"),
            OverviewStatusTone::Approval,
        ),
        RunOverviewAttentionKind::Takeover => (
            ui::i18n::text("drawers.awaiting_takeover"),
            OverviewStatusTone::Approval,
        ),
        RunOverviewAttentionKind::Review => (
            ui::i18n::text("drawers.result_review"),
            OverviewStatusTone::Review,
        ),
        RunOverviewAttentionKind::File => (
            ui::i18n::text("drawers.file_review"),
            OverviewStatusTone::File,
        ),
    };
    let enabled = match row.kind {
        RunOverviewAttentionKind::Failed | RunOverviewAttentionKind::Takeover => true,
        RunOverviewAttentionKind::Approval => {
            row.loaded && !row.run_id.is_empty() && !row.turn_id.is_empty()
        }
        RunOverviewAttentionKind::Review | RunOverviewAttentionKind::File => true,
    };
    let action = match row.kind {
        // 未加载项目里没有卡片可定位，只能先切换过去。
        RunOverviewAttentionKind::Failed | RunOverviewAttentionKind::Takeover if !row.loaded => {
            OverviewAction::SwitchProject {
                area_id: row.area_id.clone(),
            }
        }
        RunOverviewAttentionKind::Failed | RunOverviewAttentionKind::Takeover => {
            OverviewAction::LocateAgent {
                area_id: row.area_id.clone(),
                node_id: row.node_id,
            }
        }
        RunOverviewAttentionKind::Approval => OverviewAction::LocateApproval {
            area_id: row.area_id.clone(),
            node_id: row.node_id,
            run_id: row.run_id.clone(),
            turn_id: row.turn_id.clone(),
            request_id: row.request_id.clone(),
        },
        RunOverviewAttentionKind::Review => OverviewAction::OpenReview {
            area_id: row.area_id.clone(),
            direct_diff: row.paths.len() == 1,
            paths: row.paths.clone(),
        },
        RunOverviewAttentionKind::File => OverviewAction::OpenFile {
            area_id: row.area_id.clone(),
            node_id: row.node_id,
            path: row.paths.first().cloned().unwrap_or_default(),
            fingerprint: row.fingerprint.clone().unwrap_or_default(),
        },
    };
    let action_label = match row.kind {
        RunOverviewAttentionKind::Failed | RunOverviewAttentionKind::Takeover if !row.loaded => {
            ui::i18n::text("drawers.switch")
        }
        RunOverviewAttentionKind::Failed
        | RunOverviewAttentionKind::Takeover
        | RunOverviewAttentionKind::Approval => ui::i18n::text("drawers.locate"),
        RunOverviewAttentionKind::Review | RunOverviewAttentionKind::File => {
            ui::i18n::text("drawers.open")
        }
    };
    OverviewRow {
        primary: format!("{prefix} · {}", row.title),
        secondary: format!("{} · {}", row.agent, row.project),
        detail: None,
        task_line: None,
        project_tag: None,
        is_agent: false,
        action_label: action_label.into(),
        action,
        status_label: Some(prefix.into()),
        lead_label: None,
        status_tone: tone,
        approval_badge: None,
        progress: None,
        prelight: Some((row.area_id.clone(), row.node_id)),
        enabled,
        height: overview_row_height(OverviewRowKind::Attention),
    }
}
