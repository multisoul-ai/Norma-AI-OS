//! 想法清单的行视图模型：只做分组与文案，不碰 GPUI、存储或时钟。

use storage::StoredIdea;
use workspace::ideas::{IdeaState, TaskPhase};

#[cfg(test)]
#[path = "../../../tests/unit/desktop_ideas_rows_tests.rs"]
mod tests;

/// 想法栏顶部分段按钮的三个视图，一次只显示一类。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum IdeasTab {
    #[default]
    Todo,
    Assigned,
    Done,
}

impl IdeasTab {
    pub(crate) const ALL: [IdeasTab; 3] = [IdeasTab::Todo, IdeasTab::Assigned, IdeasTab::Done];

    pub(crate) fn label(self) -> &'static str {
        match self {
            IdeasTab::Todo => "待办",
            IdeasTab::Assigned => "已指派",
            IdeasTab::Done => "已完成",
        }
    }

    pub(crate) fn key(self) -> &'static str {
        match self {
            IdeasTab::Todo => "todo",
            IdeasTab::Assigned => "assigned",
            IdeasTab::Done => "done",
        }
    }
}

/// 列表里标题的字符上限（按字符数，不是字节数）。
const TITLE_CHARS: usize = 40;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum IdeaStatus {
    /// 正在投递，派发回执尚未返回。
    Sending,
    AssignedTo {
        name: String,
        /// 来自协调器的真实任务阶段；查不到时为 None（例如项目未加载）。
        phase: Option<TaskPhase>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TodoRow {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) body: String,
    pub(super) meta: String,
    /// 正文里的 http(s) 链接，按出现顺序。
    pub(super) links: Vec<String>,
    /// 附带图片的文件名，路径由存储解析。
    pub(super) image_names: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AssignedRow {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) status: IdeaStatus,
    pub(super) area_id: Option<String>,
    pub(super) node_id: Option<u64>,
    /// 任务失败时可退回待办重试。
    pub(super) retryable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DoneRow {
    pub(super) id: String,
    pub(super) title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct IdeasScreen {
    pub(super) subtitle: String,
    pub(super) badge: Option<usize>,
    pub(super) todo: Vec<TodoRow>,
    pub(super) assigned: Vec<AssignedRow>,
    pub(super) done: Vec<DoneRow>,
}

/// `agent_name` 按（项目区域，节点）查执行者名字；找不到时显示“智能体”。
/// `task_phase` 按 task_ref 查协调器里的任务阶段。
pub(super) fn ideas_screen(
    ideas: &[StoredIdea],
    now_unix: u64,
    agent_name: &dyn Fn(&str, u64) -> Option<String>,
    task_phase: &dyn Fn(&str) -> Option<TaskPhase>,
) -> IdeasScreen {
    let mut screen = IdeasScreen {
        subtitle: String::new(),
        badge: None,
        todo: Vec::new(),
        assigned: Vec::new(),
        done: Vec::new(),
    };
    for idea in ideas {
        match idea.state {
            IdeaState::Todo => screen.todo.push(TodoRow {
                id: idea.id.clone(),
                title: title_of(idea),
                body: idea.body.clone(),
                links: workspace::ideas::extract_links(&idea.body),
                image_names: idea.images.clone(),
                meta: format!(
                    "{} · {}",
                    relative_age(idea.created_at_unix, now_unix),
                    source_label(idea.source)
                ),
            }),
            IdeaState::Dispatching | IdeaState::Assigned => {
                let phase = idea.task_ref.as_deref().and_then(task_phase);
                let status = if idea.state == IdeaState::Dispatching {
                    IdeaStatus::Sending
                } else {
                    let name = idea
                        .assignee_area_id
                        .as_deref()
                        .zip(idea.assignee_node_id)
                        .and_then(|(area, node)| agent_name(area, node))
                        .unwrap_or_else(|| "智能体".to_owned());
                    IdeaStatus::AssignedTo { name, phase }
                };
                screen.assigned.push(AssignedRow {
                    id: idea.id.clone(),
                    title: title_of(idea),
                    status,
                    area_id: idea.assignee_area_id.clone(),
                    node_id: idea.assignee_node_id,
                    retryable: idea.state == IdeaState::Assigned
                        && phase == Some(TaskPhase::Failed),
                });
            }
            IdeaState::Done => screen.done.push(DoneRow {
                id: idea.id.clone(),
                title: title_of(idea),
            }),
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

/// 刷新后条数可能变少：过期的键盘光标直接失效。
pub(super) fn clamp_cursor(cursor: Option<usize>, total: usize) -> Option<usize> {
    cursor.filter(|index| *index < total)
}

fn title_of(idea: &StoredIdea) -> String {
    let first = idea
        .body
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    if first.is_empty() && !idea.images.is_empty() {
        return "图片".to_owned();
    }
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
