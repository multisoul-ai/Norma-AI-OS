//! 想法与任务状态的后台同步：只依赖 `TaskLookup` 抽象，真实实现读协调器快照与各项目的派发库。
//! 在后台线程运行；重启后协调器是空的，完成与对账因此必须能回落到派发库。

use std::collections::HashSet;
use std::path::PathBuf;
use storage::{
    AgentDispatchTaskState, AgentDispatchTaskStore, AgentDispatchTracking, IdeasStore, Reconcile,
    StorageResult, StoredIdea,
};
use workspace::ideas::TaskPhase;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_ideas_sync_tests.rs"]
mod tests;

/// 停留超过这么久的“派发中”才会被对账，避免抢先回退仍在发送的想法。
pub(super) const STUCK_AFTER_SECS: u64 = 120;
/// 已完成的想法保留的时长（7 天）。
const DONE_KEEP_SECS: u64 = 7 * 24 * 3600;

pub(super) trait TaskLookup {
    /// 按 task_ref 查任务阶段；查不到返回 None（不猜测）。
    fn phase_of(&self, task_ref: &str) -> Option<TaskPhase>;
    /// 按正文找已发出的任务；`owned` 里的引用已属于别的想法，不能再认领。
    fn find_sent(&self, body: &str, owned: &HashSet<String>) -> Reconcile;
}

/// 先找回占位引用、再对账“派发中”、再回写阶段、最后清理，返回最新清单。
pub(super) fn sync_ideas(
    store: &IdeasStore,
    workspace_id: &str,
    lookup: &dyn TaskLookup,
    now_unix: u64,
    stuck_after_secs: u64,
) -> StorageResult<Vec<StoredIdea>> {
    let ideas = store.list(workspace_id)?;
    let owned: HashSet<String> = ideas
        .iter()
        .filter_map(|idea| idea.task_ref.clone())
        .filter(|task_ref| !task_ref.starts_with("idea:"))
        .collect();
    for idea in ideas.iter().filter(|idea| {
        idea.state == workspace::ideas::IdeaState::Assigned
            && idea
                .task_ref
                .as_deref()
                .is_some_and(|task_ref| task_ref.starts_with("idea:"))
    }) {
        if let Reconcile::Sent(task_ref) = lookup.find_sent(&idea.body, &owned) {
            // 引用冲突只影响这一条，不中止整批同步。
            let _ = store.adopt_task_ref(workspace_id, &idea.id, &task_ref);
        }
    }
    store.reconcile_dispatching_with(workspace_id, stuck_after_secs, |idea| {
        lookup.find_sent(&idea.body, &owned)
    })?;
    store.apply_task_phases(workspace_id, |task_ref| lookup.phase_of(task_ref))?;
    store.purge_done_before(workspace_id, now_unix.saturating_sub(DONE_KEEP_SECS))?;
    store.list(workspace_id)
}

/// 真实查找：先看已加载项目协调器里的活动任务，再回落到各项目的派发库。
pub(super) struct DispatchLookup {
    /// 协调器快照：(task_ref, 任务正文, 阶段)。
    pub(super) live: Vec<(String, String, TaskPhase)>,
    pub(super) project_dbs: Vec<PathBuf>,
    pub(super) unavailable_projects: bool,
}

impl DispatchLookup {
    /// Background-only: a failed legacy import must not hide the workspace's idea list.
    pub(super) fn prepare_projects(&mut self, paths: &storage::DatabasePaths, roots: &[PathBuf]) {
        for root in roots {
            if paths.project_db_path(root).is_file() {
                continue;
            }
            if let Err(error) = storage::ProjectStore::for_database_paths(paths, root) {
                self.unavailable_projects = true;
                tracing::warn!(%error, path = %root.display(), "project history unavailable during idea sync");
            }
        }
    }
}

impl TaskLookup for DispatchLookup {
    fn phase_of(&self, task_ref: &str) -> Option<TaskPhase> {
        if let Some((_, _, phase)) = self
            .live
            .iter()
            .find(|(candidate, _, _)| candidate == task_ref)
        {
            return Some(*phase);
        }
        self.project_dbs.iter().find_map(|path| {
            let task = AgentDispatchTaskStore::new(path.clone())
                .get_by_task_ref(task_ref)
                .ok()??;
            Some(match (task.state, task.tracking) {
                (AgentDispatchTaskState::Done, _) => TaskPhase::Succeeded,
                (AgentDispatchTaskState::Failed | AgentDispatchTaskState::Interrupted, _) => {
                    TaskPhase::Failed
                }
                (AgentDispatchTaskState::Cancelled, _) => TaskPhase::Cancelled,
                (AgentDispatchTaskState::Untracked, _) => TaskPhase::Untracked,
                (_, AgentDispatchTracking::Untracked) => TaskPhase::Untracked,
                (AgentDispatchTaskState::Queued, _) => TaskPhase::Queued,
                _ => TaskPhase::Running,
            })
        })
    }

    fn find_sent(&self, body: &str, owned: &HashSet<String>) -> Reconcile {
        if let Some((task_ref, _, _)) = self
            .live
            .iter()
            .rev()
            .find(|(task_ref, text, _)| text == body && !owned.contains(task_ref))
        {
            return Reconcile::Sent(task_ref.clone());
        }
        for path in &self.project_dbs {
            if !path.try_exists().unwrap_or(false) {
                continue;
            }
            let Ok(tasks) = AgentDispatchTaskStore::new(path.clone()).list() else {
                // 读不出来就不能断言“没发出”，保持派发中。
                return Reconcile::Unknown;
            };
            if let Some(task) = tasks
                .iter()
                .rev()
                .find(|task| task.original_instruction == body && !owned.contains(&task.task_ref))
            {
                return Reconcile::Sent(task.task_ref.clone());
            }
        }
        if self.unavailable_projects {
            Reconcile::Unknown
        } else {
            Reconcile::NotSent
        }
    }
}
