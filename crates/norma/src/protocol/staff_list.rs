//! 幕僚任务列表投影。计数和可见性都不碰 GPUI。
use super::{Task, TaskDisplayStatus};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StaffChipCounts {
    pub all: usize,
    pub needs_you: usize,
    pub running: usize,
    /// 工作空间里全部已归档任务，不受项目筛选影响。
    pub archived: usize,
}

pub fn staff_chip_counts<'a>(
    tasks: impl IntoIterator<Item = &'a Task>,
    project_matches: impl Fn(&Task) -> bool,
) -> StaffChipCounts {
    let mut counts = StaffChipCounts::default();
    for task in tasks {
        if task.archived {
            counts.archived += 1;
            continue;
        }
        if !project_matches(task) {
            continue;
        }
        counts.all += 1;
        match task.display_status() {
            TaskDisplayStatus::NeedsYou => counts.needs_you += 1,
            TaskDisplayStatus::Running => counts.running += 1,
            TaskDisplayStatus::Waiting | TaskDisplayStatus::Merging | TaskDisplayStatus::Done => {}
        }
    }
    counts
}

pub fn staff_visible_tasks<'a>(
    tasks: &'a [Task],
    archived_view: bool,
    status: Option<TaskDisplayStatus>,
    query: &str,
    project_matches: impl Fn(&Task) -> bool,
) -> Vec<&'a Task> {
    let mut visible: Vec<&Task> = tasks
        .iter()
        .filter(|task| task.archived == archived_view)
        .filter(|task| project_matches(task))
        .filter(|task| task_matches_query(task, query))
        .filter(|task| archived_view || status.is_none_or(|status| task.display_status() == status))
        .collect();
    visible.sort_by(|left, right| {
        status_rank(left.display_status())
            .cmp(&status_rank(right.display_status()))
            .then(right.updated_at.cmp(&left.updated_at))
    });
    visible
}

fn task_matches_query(task: &Task, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || task.title.to_lowercase().contains(&query)
        || task
            .workers
            .iter()
            .any(|worker| worker.agent.to_lowercase().contains(&query))
}

fn status_rank(status: TaskDisplayStatus) -> u8 {
    match status {
        TaskDisplayStatus::NeedsYou => 0,
        TaskDisplayStatus::Running => 1,
        TaskDisplayStatus::Waiting => 2,
        TaskDisplayStatus::Merging => 3,
        TaskDisplayStatus::Done => 4,
    }
}
