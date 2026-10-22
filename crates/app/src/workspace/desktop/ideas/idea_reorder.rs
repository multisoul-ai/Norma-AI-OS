//! 待办想法排序：拖动与 `⌥↑/⌥↓` 共用。纯函数决定“移到谁之前”，宿主先改内存再后台写库，
//! 鼠标移动路径不碰存储；失败时刷新会把清单恢复为库里的真实顺序。

use crate::app::MultisoulApp;
use gpui::{AppContext as _, Context};
use storage::{IdeasStore, StoredIdea};
use workspace::ideas::IdeaState;

#[cfg(test)]
#[path = "../../../../tests/unit/desktop_idea_reorder_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::workspace::desktop) enum Step {
    Up,
    Down,
}

/// `⌥↑/⌥↓` 把第 `index` 条待办移动一格：返回（移到谁之前，移动后的光标位置）；到头了返回 `None`。
pub(in crate::workspace::desktop) fn keyboard_step(
    todo_ids: &[String],
    index: usize,
    step: Step,
) -> Option<(Option<String>, usize)> {
    if index >= todo_ids.len() {
        return None;
    }
    match step {
        Step::Up => {
            let target = index.checked_sub(1)?;
            Some((Some(todo_ids[target].clone()), target))
        }
        Step::Down => {
            if index + 1 >= todo_ids.len() {
                return None;
            }
            // 越过下一条：放到“下下条”之前；下一条已是末尾则放到末尾。
            Some((todo_ids.get(index + 2).cloned(), index + 1))
        }
    }
}

/// 在内存清单里把待办 `id` 移到 `before` 之前（`None` 为所有待办之后），与库里 `move_before` 同语义。
/// 返回是否发生了变化。已指派等非待办条目的相对位置不变。
pub(super) fn apply_move(ideas: &mut Vec<StoredIdea>, id: &str, before: Option<&str>) -> bool {
    if before == Some(id) {
        return false;
    }
    let is_todo = |idea: &StoredIdea| idea.state == IdeaState::Todo;
    let Some(from) = ideas.iter().position(|idea| idea.id == id && is_todo(idea)) else {
        return false;
    };
    if let Some(target) = before {
        if !ideas.iter().any(|idea| idea.id == target && is_todo(idea)) {
            return false;
        }
    }
    let item = ideas.remove(from);
    let slot = match before {
        Some(target) => ideas.iter().position(|idea| idea.id == target),
        None => ideas
            .iter()
            .rposition(is_todo)
            .map(|last| last + 1)
            .or(Some(from.min(ideas.len()))),
    }
    .unwrap_or(from);
    let unchanged = slot == from;
    ideas.insert(slot, item);
    !unchanged
}

impl MultisoulApp {
    /// 先在内存里改顺序让界面立即生效，再后台写库并刷新；写库失败时刷新会恢复真实顺序。
    pub(in crate::workspace::desktop) fn reorder_idea(
        &mut self,
        id: &str,
        before: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        if !apply_move(&mut self.desktop_workspace.ideas, id, before) {
            return;
        }
        let store = IdeasStore::new(self.database_paths.app_db_path().to_owned());
        let (id, before) = (id.to_owned(), before.map(str::to_owned));
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    IdeasStore::move_before(&store, &workspace_id, &id, before.as_deref())
                })
                .await;
            if let Err(error) = result {
                tracing::warn!(%error, "failed to persist idea order");
            }
            let _ = this.update(cx, |app, cx| app.refresh_ideas(cx));
        })
        .detach();
        cx.notify();
    }
}
