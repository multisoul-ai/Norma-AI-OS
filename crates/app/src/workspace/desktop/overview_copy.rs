//! Copy and segment colors for run-overview rows. Kept apart so the row
//! module stays under the file-length cap.

use super::{OverviewStatusTone, ProgressMark};
use norma::mobile::{RunOverviewProject, TaskProgress, WorkspaceActivityStatus};

pub(super) fn progress_detail(
    progress: &TaskProgress,
    started: Option<u64>,
    now_ms: u64,
) -> (String, Vec<ProgressMark>) {
    let total = progress.total;
    let done = progress.completed.min(total);
    let marks = (0..total)
        .map(|index| {
            if index < done {
                ProgressMark::Done
            } else if done < total && index == done {
                ProgressMark::Current
            } else {
                ProgressMark::Todo
            }
        })
        .collect();
    let elapsed = started.map(|started| elapsed_label(started, now_ms));
    let text = if done < total {
        let item = ui::i18n::format(
            "drawers.progress_step",
            &[
                ("arg0", (done + 1).to_string()),
                ("arg1", (total).to_string()),
            ],
        );
        let step = progress
            .current_step
            .as_deref()
            .map(str::trim)
            .filter(|step| !step.is_empty());
        match (step, elapsed) {
            (Some(step), Some(elapsed)) => format!("{item} · {step} · {elapsed}"),
            (Some(step), None) => format!("{item} · {step}"),
            (None, Some(elapsed)) => format!("{item} · {elapsed}"),
            (None, None) => item,
        }
    } else {
        match elapsed {
            Some(elapsed) => ui::i18n::format(
                "drawers.completed_elapsed",
                &[
                    ("total", (total).to_string()),
                    ("elapsed", (elapsed).to_string()),
                ],
            ),
            None => ui::i18n::format("drawers.completed_count", &[("total", (total).to_string())]),
        }
    };
    (text, marks)
}

pub(in crate::workspace::desktop) fn running_detail(
    action: Option<&str>,
    started: Option<u64>,
    now_ms: u64,
) -> String {
    let mut parts = vec![ui::i18n::text("drawers.running").to_string()];
    if let Some(action) = action.map(str::trim).filter(|action| !action.is_empty()) {
        parts.push(action.to_string());
    }
    if let Some(started) = started {
        parts.push(elapsed_label(started, now_ms));
    }
    parts.join(" · ")
}

pub(super) fn project_secondary(project: &RunOverviewProject, now_ms: u64) -> String {
    let mut text = ui::i18n::format(
        "drawers.project_status",
        &[
            ("arg0", (project.agent_count).to_string()),
            ("arg1", (project_status(project.status)).to_string()),
        ],
    );
    if let Some(updated) = project.updated_at_unix_ms {
        let when = relative_update(updated, now_ms);
        if !when.is_empty() {
            text.push_str(" · ");
            text.push_str(&when);
        }
    }
    if !project.loaded {
        text.push_str(ui::i18n::text("drawers.not_loaded_suffix"));
    }
    text
}

pub(super) fn project_status(status: WorkspaceActivityStatus) -> &'static str {
    match status {
        WorkspaceActivityStatus::Approval => ui::i18n::text("drawers.pending_approval"),
        WorkspaceActivityStatus::Failed => ui::i18n::text("drawers.error"),
        WorkspaceActivityStatus::Review => ui::i18n::text("drawers.pending_review"),
        WorkspaceActivityStatus::Running => ui::i18n::text("drawers.running"),
        WorkspaceActivityStatus::Idle => ui::i18n::text("drawers.idle"),
    }
}

pub(super) fn project_tone(status: WorkspaceActivityStatus) -> OverviewStatusTone {
    match status {
        WorkspaceActivityStatus::Approval => OverviewStatusTone::Approval,
        WorkspaceActivityStatus::Failed => OverviewStatusTone::Failed,
        WorkspaceActivityStatus::Review => OverviewStatusTone::Review,
        WorkspaceActivityStatus::Running => OverviewStatusTone::Running,
        WorkspaceActivityStatus::Idle => OverviewStatusTone::Idle,
    }
}

fn elapsed_label(started_at_unix_ms: u64, now_ms: u64) -> String {
    let minutes = now_ms.saturating_sub(started_at_unix_ms) / 60_000;
    if minutes == 0 {
        ui::i18n::text("drawers.running_under_minute").into()
    } else {
        ui::i18n::format(
            "drawers.running_minutes",
            &[("minutes", (minutes).to_string())],
        )
    }
}

pub(super) fn relative_update(updated_at_unix_ms: i64, now_ms: u64) -> String {
    if updated_at_unix_ms < 0 {
        return String::new();
    }
    let age = now_ms.saturating_sub(updated_at_unix_ms as u64) / 1_000;
    match age {
        0..=59 => ui::i18n::text("drawers.just_now").into(),
        60..=3_599 => ui::i18n::format("drawers.minutes_ago", &[("arg0", (age / 60).to_string())]),
        3_600..=86_399 => {
            ui::i18n::format("drawers.hours_ago", &[("arg0", (age / 3_600).to_string())])
        }
        _ => ui::i18n::format("drawers.days_ago", &[("arg0", (age / 86_400).to_string())]),
    }
}

/// 概览副标题：工作空间名，有投影时间时追加「· 刚刚更新」。
pub(in crate::workspace::desktop) fn overview_subtitle(
    name: &str,
    updated_at_ms: Option<i64>,
    now_ms: u64,
) -> String {
    match updated_at_ms.map(|at| relative_update(at, now_ms)) {
        Some(when) if !when.is_empty() && !name.is_empty() => {
            format!("{name} · {}", when_updated(&when))
        }
        _ => name.to_string(),
    }
}

fn when_updated(when: &str) -> String {
    if when == ui::i18n::text("drawers.just_now") {
        ui::i18n::text("drawers.updated_now").into()
    } else {
        ui::i18n::format("drawers.updated_when", &[("when", (when).to_string())])
    }
}

/// Translate structured runtime actions, preserving the provider's tool name.
pub(in crate::workspace::desktop) fn localized_live_action(
    state: &runtime::AgentTaskState,
) -> Option<String> {
    match state {
        runtime::AgentTaskState::Thinking => Some(ui::i18n::text("drawers.thinking").into()),
        runtime::AgentTaskState::RunningTool { tool_name } => {
            Some(match tool_name.as_deref().filter(|name| !name.is_empty()) {
                Some(tool) => {
                    ui::i18n::format("drawers.calling_tool", &[("tool", tool.to_string())])
                }
                None => ui::i18n::text("drawers.running_tool").into(),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/drawer_i18n_tests.rs"]
mod drawer_i18n_tests;
