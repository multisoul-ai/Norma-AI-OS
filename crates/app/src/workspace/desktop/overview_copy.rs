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
        let item = format!("第 {}/{} 项", done + 1, total);
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
            Some(elapsed) => format!("已完成 {total} 项 · {elapsed}"),
            None => format!("已完成 {total} 项"),
        }
    };
    (text, marks)
}

pub(super) fn running_detail(action: Option<&str>, started: Option<u64>, now_ms: u64) -> String {
    let mut parts = vec!["运行中".to_string()];
    if let Some(action) = action.map(str::trim).filter(|action| !action.is_empty()) {
        parts.push(action.to_string());
    }
    if let Some(started) = started {
        parts.push(elapsed_label(started, now_ms));
    }
    parts.join(" · ")
}

pub(super) fn project_secondary(project: &RunOverviewProject, now_ms: u64) -> String {
    let mut text = format!(
        "{} 个智能体 · 上次状态 {}",
        project.agent_count,
        project_status(project.status)
    );
    if let Some(updated) = project.updated_at_unix_ms {
        let when = relative_update(updated, now_ms);
        if !when.is_empty() {
            text.push_str(" · ");
            text.push_str(&when);
        }
    }
    if !project.loaded {
        text.push_str(" · 未加载");
    }
    text
}

pub(super) fn project_status(status: WorkspaceActivityStatus) -> &'static str {
    match status {
        WorkspaceActivityStatus::Approval => "待审核",
        WorkspaceActivityStatus::Failed => "出错",
        WorkspaceActivityStatus::Review => "待查看",
        WorkspaceActivityStatus::Running => "运行中",
        WorkspaceActivityStatus::Idle => "空闲",
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
        "已运行不到 1 分".into()
    } else {
        format!("已运行 {minutes} 分")
    }
}

pub(super) fn relative_update(updated_at_unix_ms: i64, now_ms: u64) -> String {
    if updated_at_unix_ms < 0 {
        return String::new();
    }
    let age = now_ms.saturating_sub(updated_at_unix_ms as u64) / 1_000;
    match age {
        0..=59 => "刚刚".into(),
        60..=3_599 => format!("{} 分钟前", age / 60),
        3_600..=86_399 => format!("{} 小时前", age / 3_600),
        _ => format!("{} 天前", age / 86_400),
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
    if when == "刚刚" {
        "刚刚更新".into()
    } else {
        format!("{when}更新")
    }
}
