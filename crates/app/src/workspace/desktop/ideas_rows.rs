//! 想法清单的行视图模型：只做分组与文案，不碰 GPUI、存储或时钟。

use storage::StoredIdea;
use workspace::ideas::IdeaState;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_ideas_rows_tests.rs"]
mod tests;

/// 列表里标题的字符上限（按字符数，不是字节数）。
const TITLE_CHARS: usize = 40;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum IdeaStatus {
    /// 正在投递，派发回执尚未返回。
    Sending,
    AssignedTo(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TodoRow {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) body: String,
    pub(super) meta: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AssignedRow {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) status: IdeaStatus,
    pub(super) area_id: Option<String>,
    pub(super) node_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct IdeasScreen {
    pub(super) subtitle: String,
    pub(super) badge: Option<usize>,
    pub(super) todo: Vec<TodoRow>,
    pub(super) assigned: Vec<AssignedRow>,
    pub(super) done_count: usize,
}

/// `agent_name` 按（项目区域，节点）查执行者名字；找不到时显示“智能体”。
pub(super) fn ideas_screen(
    ideas: &[StoredIdea],
    now_unix: u64,
    agent_name: &dyn Fn(&str, u64) -> Option<String>,
) -> IdeasScreen {
    let mut screen = IdeasScreen {
        subtitle: String::new(),
        badge: None,
        todo: Vec::new(),
        assigned: Vec::new(),
        done_count: 0,
    };
    for idea in ideas {
        match idea.state {
            IdeaState::Todo => screen.todo.push(TodoRow {
                id: idea.id.clone(),
                title: title_of(&idea.body),
                body: idea.body.clone(),
                meta: format!(
                    "{} · {}",
                    relative_age(idea.created_at_unix, now_unix),
                    source_label(idea.source)
                ),
            }),
            IdeaState::Dispatching | IdeaState::Assigned => {
                let status = if idea.state == IdeaState::Dispatching {
                    IdeaStatus::Sending
                } else {
                    let name = idea
                        .assignee_area_id
                        .as_deref()
                        .zip(idea.assignee_node_id)
                        .and_then(|(area, node)| agent_name(area, node))
                        .unwrap_or_else(|| "智能体".to_owned());
                    IdeaStatus::AssignedTo(name)
                };
                screen.assigned.push(AssignedRow {
                    id: idea.id.clone(),
                    title: title_of(&idea.body),
                    status,
                    area_id: idea.assignee_area_id.clone(),
                    node_id: idea.assignee_node_id,
                });
            }
            IdeaState::Done => screen.done_count += 1,
            IdeaState::Dismissed => {}
        }
    }
    screen.badge = (!screen.todo.is_empty()).then_some(screen.todo.len());
    screen.subtitle = if screen.todo.is_empty() {
        "对 Norma 说“记一下…”，她会帮你记下".to_owned()
    } else {
        format!("{} 条待办 · 拖到智能体卡片，或点“指派”", screen.todo.len())
    };
    screen
}

fn title_of(body: &str) -> String {
    let first = body.lines().next().unwrap_or_default().trim();
    if first.chars().count() > TITLE_CHARS {
        let cut: String = first.chars().take(TITLE_CHARS).collect();
        format!("{cut}…")
    } else {
        first.to_owned()
    }
}

fn source_label(source: storage::IdeaSource) -> &'static str {
    match source {
        storage::IdeaSource::Chat => "来自对话",
        storage::IdeaSource::Voice => "来自语音",
        storage::IdeaSource::Manual => "手动添加",
    }
}

pub(super) fn relative_age(created_unix: u64, now_unix: u64) -> String {
    let seconds = now_unix.saturating_sub(created_unix);
    match seconds {
        0..=59 => "刚刚".to_owned(),
        60..=3_599 => format!("{} 分钟前", seconds / 60),
        3_600..=86_399 => format!("{} 小时前", seconds / 3_600),
        _ => format!("{} 天前", seconds / 86_400),
    }
}
