//! 想法的手机端只读视图：截断后的标题与粗粒度状态，不含正文、路径与任务引用。

use crate::{IdeasStore, StorageResult};
use workspace::ideas::IdeaState;

/// 手机端最多展示的想法条数与标题字符数。
const MOBILE_IDEA_LIMIT: usize = 100;
const MOBILE_TITLE_CHARS: usize = 80;

/// 手机端看到的一条想法。
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct MobileIdea {
    pub id: String,
    pub title: String,
    /// `todo` / `assigned` / `done`。
    pub state: &'static str,
    pub created_at_unix: u64,
}

fn mobile_title(body: &str) -> String {
    let cleaned: String = body
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    let first = cleaned.trim();
    if first.chars().count() > MOBILE_TITLE_CHARS {
        let cut: String = first.chars().take(MOBILE_TITLE_CHARS).collect();
        format!("{cut}…")
    } else {
        first.to_owned()
    }
}

impl IdeasStore {
    /// 手机端只读视图：截断后的标题与粗粒度状态，不含正文、路径与任务引用。
    pub fn mobile_view(&self, workspace_id: &str) -> StorageResult<Vec<MobileIdea>> {
        let mut ideas = self.list(workspace_id)?;
        // 超出上限时先丢已完成、再丢已指派，最新的待办始终可见（排序稳定，各组内保持记录顺序）。
        ideas.sort_by_key(|idea| match idea.state {
            IdeaState::Todo => 0,
            IdeaState::Dispatching | IdeaState::Assigned => 1,
            _ => 2,
        });
        Ok(ideas
            .into_iter()
            .take(MOBILE_IDEA_LIMIT)
            .map(|idea| MobileIdea {
                title: mobile_title(&idea.body),
                state: match idea.state {
                    IdeaState::Todo => "todo",
                    IdeaState::Dispatching | IdeaState::Assigned => "assigned",
                    IdeaState::Done | IdeaState::Dismissed => "done",
                },
                id: idea.id,
                created_at_unix: idea.created_at_unix,
            })
            .collect())
    }
}
