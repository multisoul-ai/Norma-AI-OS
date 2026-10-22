//! 想法清单的清空与撤销：删除、清空「已指派」、清空「已完成」都走同一个撤销栈。
//!
//! - 删除是软删除（可恢复）；清空「已完成」在撤销窗口内只是不显示，窗口过后才真正落库删除。
//! - 清空「已指派」只收进已完成那些不再运行的任务，仍在运行或排队的保留。

use crate::app::MultisoulApp;
use gpui::{AppContext as _, Context};
use std::time::Duration;
use storage::IdeasStore;

/// 删除后可撤销的时间窗口。
pub(super) const UNDO_WINDOW: Duration = Duration::from_secs(6);

/// 一次可撤销的操作。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum IdeaUndo {
    /// 删除了一条想法（已软删除，可恢复）。
    Delete { id: String, title: String },
    /// 清空了「已完成」：撤销窗口过后才真正删除这些想法。
    ClearDone {
        workspace_id: String,
        ids: Vec<String>,
    },
}

impl IdeaUndo {
    /// 撤销提示条上的文字。
    pub(crate) fn message(&self) -> String {
        match self {
            Self::Delete { title, .. } => format!("已删除「{title}」"),
            Self::ClearDone { ids, .. } => format!("已清空 {} 条已完成", ids.len()),
        }
    }
}

/// 撤销栈最多保留几条。
const UNDO_DEPTH: usize = 5;

impl MultisoulApp {
    /// 清空“已指派”：把不再运行的收进已完成；仍在运行或排队的任务保留，不把它们标成完成。
    pub(super) fn clear_assigned_ideas(&mut self, cx: &mut Context<Self>) {
        use workspace::ideas::TaskPhase;
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let ids: Vec<String> = self
            .ideas_screen(cx)
            .assigned
            .into_iter()
            .filter(|row| {
                matches!(
                    &row.status,
                    super::ideas_rows::IdeaStatus::AssignedTo { phase, .. }
                        if !matches!(phase, Some(TaskPhase::Running | TaskPhase::Queued))
                )
            })
            .map(|row| row.id)
            .collect();
        if ids.is_empty() {
            self.desktop_workspace.ideas_notice =
                Some("没有可清空的：已指派的任务仍在运行或排队".to_owned());
            cx.notify();
            return;
        }
        for idea in &mut self.desktop_workspace.ideas {
            if ids.contains(&idea.id) && idea.state == workspace::ideas::IdeaState::Assigned {
                idea.state = workspace::ideas::IdeaState::Done;
            }
        }
        self.desktop_workspace.ideas_cursor = None;
        self.desktop_workspace.ideas_notice = None;
        let store = IdeasStore::new(self.database_paths.app_db_path().to_owned());
        cx.spawn(async move |this, cx| {
            let _ = cx
                .background_spawn(async move { store.complete_assigned_ids(&workspace_id, &ids) })
                .await;
            let _ = this.update(cx, |app, cx| app.refresh_ideas(cx));
        })
        .detach();
        cx.notify();
    }

    /// 清空「已完成」：立即不再显示；撤销窗口（6 秒）过后才真正从库里删除。
    pub(super) fn clear_done_ideas(&mut self, cx: &mut Context<Self>) {
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let ids: Vec<String> = self
            .desktop_workspace
            .ideas
            .iter()
            .filter(|idea| {
                idea.state == workspace::ideas::IdeaState::Done
                    && !self
                        .desktop_workspace
                        .ideas_pending_purge
                        .contains(&idea.id)
            })
            .map(|idea| idea.id.clone())
            .collect();
        if ids.is_empty() {
            return;
        }
        self.desktop_workspace
            .ideas_pending_purge
            .extend(ids.iter().cloned());
        self.push_idea_undo(IdeaUndo::ClearDone { workspace_id, ids });
        self.schedule_idea_undo_expiry(cx);
        cx.notify();
    }

    pub(super) fn push_idea_undo(&mut self, undo: IdeaUndo) {
        let stack = &mut self.desktop_workspace.ideas_undo;
        stack.push(undo);
        if stack.len() > UNDO_DEPTH {
            stack.remove(0);
        }
    }

    /// 每次有新的可撤销操作都重新计时；到点后撤销窗口关闭，清空「已完成」在此刻真正落库。
    pub(super) fn schedule_idea_undo_expiry(&mut self, cx: &mut Context<Self>) {
        self.desktop_workspace.ideas_undo_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(UNDO_WINDOW).await;
            let _ = this.update(cx, |app, cx| app.expire_idea_undo(cx));
        }));
    }

    pub(super) fn expire_idea_undo(&mut self, cx: &mut Context<Self>) {
        let expired = std::mem::take(&mut self.desktop_workspace.ideas_undo);
        let store = IdeasStore::new(self.database_paths.app_db_path().to_owned());
        for undo in expired {
            let IdeaUndo::ClearDone { workspace_id, ids } = undo else {
                continue;
            };
            let store = store.clone();
            cx.spawn(async move |this, cx| {
                let purged = ids.clone();
                let _ = cx
                    .background_spawn(async move { store.delete_done(&workspace_id, &ids) })
                    .await;
                let _ = this.update(cx, |app, cx| {
                    for id in &purged {
                        app.desktop_workspace.ideas_pending_purge.remove(id);
                    }
                    app.refresh_ideas(cx);
                });
            })
            .detach();
        }
        cx.notify();
    }

    /// 撤销最近一次操作：恢复被删除的想法，或取消清空「已完成」。
    pub(super) fn undo_last_idea_action(&mut self, cx: &mut Context<Self>) {
        let Some(undo) = self.desktop_workspace.ideas_undo.pop() else {
            return;
        };
        if self.desktop_workspace.ideas_undo.is_empty() {
            self.desktop_workspace.ideas_undo_timer = None;
        }
        match undo {
            IdeaUndo::Delete { id, .. } => {
                let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
                    return;
                };
                let store = IdeasStore::new(self.database_paths.app_db_path().to_owned());
                cx.spawn(async move |this, cx| {
                    let _ = cx
                        .background_spawn(async move { store.restore(&workspace_id, &id) })
                        .await;
                    let _ = this.update(cx, |app, cx| app.refresh_ideas(cx));
                })
                .detach();
            }
            IdeaUndo::ClearDone { ids, .. } => {
                for id in &ids {
                    self.desktop_workspace.ideas_pending_purge.remove(id);
                }
            }
        }
        cx.notify();
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/desktop_ideas_undo_tests.rs"]
mod undo_tests;
