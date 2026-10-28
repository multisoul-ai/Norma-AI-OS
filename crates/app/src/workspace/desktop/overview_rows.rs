//! Observable run-overview rows. Grouping stays in the cached projection;
//! this module only formats labels, progress segments, and keyboard movement.

use norma::mobile::{
    RunOverview, RunOverviewAgent, RunOverviewAttention, RunOverviewAttentionKind,
    RunOverviewPresence, RunOverviewProject,
};
use ui::icons::with_alpha;
use ui::Theme;

#[path = "overview_copy.rs"]
mod overview_copy;
pub(in crate::workspace::desktop) use overview_copy::overview_subtitle;
use overview_copy::{
    progress_detail, project_secondary, project_status, project_tone, running_detail,
};

#[cfg(test)]
#[path = "../../../tests/unit/run_overview_groups_tests.rs"]
mod group_tests;
#[cfg(test)]
#[path = "../../../tests/unit/run_overview_prompt_tests.rs"]
mod prompt_tests;
#[cfg(test)]
#[path = "../../../tests/unit/run_overview_rows_tests.rs"]
mod tests;

pub(super) const OVERVIEW_TEXT_PRIMARY: f32 = 13.0;
pub(super) const OVERVIEW_TEXT_SECONDARY: f32 = 11.0;
pub(super) const OVERVIEW_TEXT_COPY: f32 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OverviewRowKind {
    Attention,
    Agent { progress: bool },
    Project,
}

pub(super) fn overview_row_height(kind: OverviewRowKind) -> f32 {
    match kind {
        OverviewRowKind::Attention | OverviewRowKind::Project => 48.0,
        // 名称、项目标签行、状态行三行，比只有名称和状态时多一行。
        OverviewRowKind::Agent { progress: false } => 72.0,
        // 名称、项目标签行、步骤条、进度详情四行。
        OverviewRowKind::Agent { progress: true } => 108.0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProgressMark {
    Done,
    Current,
    Todo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OverviewStatusTone {
    Running,
    Idle,
    Failed,
    Approval,
    Review,
    File,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum OverviewAction {
    LocateApproval {
        area_id: String,
        node_id: u64,
        run_id: String,
        turn_id: String,
        /// 有请求标识才能在概览里直接允许或拒绝。
        request_id: Option<String>,
    },
    OpenReview {
        area_id: String,
        paths: Vec<String>,
        direct_diff: bool,
    },
    OpenFile {
        area_id: String,
        node_id: u64,
        path: String,
        fingerprint: String,
    },
    LocateAgent {
        area_id: String,
        node_id: u64,
    },
    StopEgo {
        area_id: String,
        node_id: u64,
        agent_id: u64,
    },
    SwitchProject {
        area_id: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct OverviewRow {
    pub primary: String,
    pub secondary: String,
    pub detail: Option<String>,
    pub task_line: Option<String>,
    /// 智能体行所属项目，渲染成任务行前的小标签；项目行与待处理行没有。
    pub project_tag: Option<String>,
    /// 智能体行（带状态角标、可含步骤条）；待处理与项目行为 false。
    pub is_agent: bool,
    pub action_label: String,
    pub action: OverviewAction,
    pub status_label: Option<String>,
    /// Project rows show their 1-based position in a small box instead of a status dot.
    pub lead_label: Option<String>,
    pub status_tone: OverviewStatusTone,
    pub approval_badge: Option<String>,
    pub progress: Option<Vec<ProgressMark>>,
    pub prelight: Option<(String, u64)>,
    pub enabled: bool,
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct OverviewSection {
    pub title: String,
    pub count: Option<usize>,
    pub rows: Vec<OverviewRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct OverviewScreen {
    pub sections: Vec<OverviewSection>,
}

impl OverviewScreen {
    pub(super) fn empty() -> Self {
        Self {
            sections: vec![
                section("需要你处理", Some(0), Vec::new()),
                section("运行中", Some(0), Vec::new()),
                section("待开始", Some(0), Vec::new()),
            ],
        }
    }

    /// 统计卡：待你处理、运行中、待开始的数量，与分组标题的计数同源。
    pub(super) fn summary(&self) -> [usize; 3] {
        let count = |index: usize| {
            self.sections
                .get(index)
                .and_then(|section| section.count)
                .unwrap_or(0)
        };
        [count(0), count(1), count(2)]
    }

    pub(super) fn row_count(&self) -> usize {
        self.sections.iter().map(|section| section.rows.len()).sum()
    }

    pub(super) fn row(&self, index: usize) -> Option<&OverviewRow> {
        self.sections
            .iter()
            .flat_map(|section| section.rows.iter())
            .nth(index)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OverviewKey {
    Up,
    Down,
    Home,
    End,
    Enter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OverviewCursor {
    Move(usize),
    Activate(usize),
}

pub(super) fn move_overview_cursor(
    cursor: Option<usize>,
    len: usize,
    key: OverviewKey,
) -> Option<OverviewCursor> {
    if len == 0 {
        return None;
    }
    let last = len - 1;
    Some(match key {
        OverviewKey::Home => OverviewCursor::Move(0),
        OverviewKey::End => OverviewCursor::Move(last),
        OverviewKey::Up => {
            OverviewCursor::Move(cursor.map(|index| index.saturating_sub(1)).unwrap_or(0))
        }
        OverviewKey::Down => {
            OverviewCursor::Move(cursor.map(|index| (index + 1).min(last)).unwrap_or(0))
        }
        OverviewKey::Enter => OverviewCursor::Activate(cursor.unwrap_or(0)),
    })
}

pub(super) fn overview_action_starts_project(action: &OverviewAction) -> bool {
    matches!(action, OverviewAction::SwitchProject { .. })
}

pub(super) fn overview_action_keeps_overview(action: &OverviewAction) -> bool {
    !overview_action_starts_project(action)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OverviewFeedback {
    pub rest: u32,
    pub hover: u32,
    pub pressed: u32,
    pub focus: u32,
}

pub(super) fn overview_feedback(theme: Theme) -> OverviewFeedback {
    OverviewFeedback {
        rest: 0x0000_0000,
        hover: with_alpha(theme.accent_blue, 0x2e),
        pressed: with_alpha(theme.accent_blue, 0x55),
        focus: theme.focused_stroke,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OverviewTone {
    pub running: u32,
    pub idle: u32,
    pub failed: u32,
    pub approval: u32,
    pub review: u32,
    pub file: u32,
}

pub(super) fn overview_tone(theme: Theme) -> OverviewTone {
    OverviewTone {
        running: theme.accent_cyan,
        idle: theme.text_tertiary,
        failed: theme.destructive,
        approval: theme.accent_warm,
        review: theme.success,
        file: theme.accent,
    }
}

pub(super) fn present_overview(
    overview: &RunOverview,
    ego_stops: &[(String, u64, u64)],
    now_ms: u64,
) -> OverviewScreen {
    let idle_agents = overview.idle.len();
    let mut idle_rows: Vec<OverviewRow> = overview
        .idle
        .iter()
        .map(|agent| agent_row(agent, ego_stops, now_ms))
        .collect();
    // 未加载项目只有上次状态，排在智能体之后，不计入“待开始”的数量。
    idle_rows.extend(
        overview
            .unloaded_projects
            .iter()
            .map(|project| project_row(project, now_ms)),
    );
    OverviewScreen {
        sections: vec![
            section(
                "需要你处理",
                Some(overview.attention.len()),
                overview.attention.iter().map(attention_row).collect(),
            ),
            section(
                "运行中",
                Some(overview.running.len()),
                overview
                    .running
                    .iter()
                    .map(|agent| agent_row(agent, ego_stops, now_ms))
                    .collect(),
            ),
            section("待开始", Some(idle_agents), idle_rows),
        ],
    }
}

fn section(
    title: impl Into<String>,
    count: Option<usize>,
    rows: Vec<OverviewRow>,
) -> OverviewSection {
    OverviewSection {
        title: title.into(),
        count,
        rows,
    }
}

fn attention_row(row: &RunOverviewAttention) -> OverviewRow {
    let (prefix, tone) = match row.kind {
        RunOverviewAttentionKind::Failed => ("出错", OverviewStatusTone::Failed),
        RunOverviewAttentionKind::Approval => ("待审核", OverviewStatusTone::Approval),
        RunOverviewAttentionKind::Takeover => ("等待接管", OverviewStatusTone::Approval),
        RunOverviewAttentionKind::Review => ("成果待查看", OverviewStatusTone::Review),
        RunOverviewAttentionKind::File => ("待查看文件", OverviewStatusTone::File),
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
            "切换"
        }
        RunOverviewAttentionKind::Failed
        | RunOverviewAttentionKind::Takeover
        | RunOverviewAttentionKind::Approval => "定位",
        RunOverviewAttentionKind::Review | RunOverviewAttentionKind::File => "打开",
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

fn agent_row(
    agent: &RunOverviewAgent,
    ego_stops: &[(String, u64, u64)],
    now_ms: u64,
) -> OverviewRow {
    let (status_label, status_tone) = match agent.presence {
        RunOverviewPresence::Running => ("运行中", OverviewStatusTone::Running),
        RunOverviewPresence::Idle => ("待开始", OverviewStatusTone::Idle),
        RunOverviewPresence::Failed => ("出错", OverviewStatusTone::Failed),
    };
    let (detail, progress) = match &agent.progress {
        Some(item) => {
            let (text, marks) = progress_detail(item, agent.started_at_unix_ms, now_ms);
            (Some(text), Some(marks))
        }
        None if agent.presence == RunOverviewPresence::Running => (
            Some(running_detail(
                agent.action.as_deref(),
                agent.started_at_unix_ms,
                now_ms,
            )),
            None,
        ),
        None if agent.presence == RunOverviewPresence::Failed => (Some("出错".into()), None),
        None => (Some("尚未开始".into()), None),
    };
    let stop = ego_stops
        .iter()
        .find(|item| item.0 == agent.area_id && item.1 == agent.node_id);
    let (action_label, action) = if let Some((_, _, agent_id)) = stop {
        (
            "停止任务",
            OverviewAction::StopEgo {
                area_id: agent.area_id.clone(),
                node_id: agent.node_id,
                agent_id: *agent_id,
            },
        )
    } else {
        (
            "定位",
            OverviewAction::LocateAgent {
                area_id: agent.area_id.clone(),
                node_id: agent.node_id,
            },
        )
    };
    // 主标题是会话名称（首条提示词），第二行是最新输入；卡片名退到项目标签里。
    let title = agent.task.as_deref().and_then(single_line);
    let latest = agent
        .latest_prompt
        .as_deref()
        .and_then(single_line)
        .filter(|latest| Some(latest) != title.as_ref());
    let project_tag = match (agent.project.is_empty(), title.is_some()) {
        (true, _) => None,
        (false, true) => Some(format!("{} · {}", agent.project, agent.name)),
        (false, false) => Some(agent.project.clone()),
    };
    OverviewRow {
        primary: title.unwrap_or_else(|| agent.name.clone()),
        secondary: String::new(),
        detail,
        task_line: latest,
        project_tag,
        is_agent: true,
        action_label: action_label.into(),
        action,
        status_label: Some(status_label.into()),
        lead_label: None,
        status_tone,
        approval_badge: (agent.approval_count > 0)
            .then(|| format!("{} 待审核", agent.approval_count)),
        progress,
        prelight: Some((agent.area_id.clone(), agent.node_id)),
        enabled: true,
        height: overview_row_height(OverviewRowKind::Agent {
            progress: agent.progress.is_some(),
        }),
    }
}

/// 提示词可能含换行和多余空白，概览里折成单行再交给视图截断。
fn single_line(text: &str) -> Option<String> {
    let folded = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!folded.is_empty()).then_some(folded)
}

fn project_row(project: &RunOverviewProject, now_ms: u64) -> OverviewRow {
    OverviewRow {
        primary: project.name.clone(),
        secondary: project_secondary(project, now_ms),
        detail: None,
        task_line: None,
        project_tag: None,
        is_agent: false,
        action_label: "切换".into(),
        action: OverviewAction::SwitchProject {
            area_id: project.area_id.clone(),
        },
        status_label: Some(project_status(project.status).into()),
        lead_label: Some(format!("{:02}", project.order)),
        status_tone: project_tone(project.status),
        approval_badge: None,
        progress: None,
        prelight: None,
        enabled: true,
        height: overview_row_height(OverviewRowKind::Project),
    }
}
