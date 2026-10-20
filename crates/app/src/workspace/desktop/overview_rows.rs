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
use overview_copy::{
    progress_detail, project_secondary, project_status, project_tone, running_detail,
};

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
        OverviewRowKind::Agent { progress: false } => 64.0,
        OverviewRowKind::Agent { progress: true } => 86.0,
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
    pub meta: Option<String>,
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
                section("需要你处理", None, Some(0), Vec::new()),
                section("当前项目", Some("0 个智能体".into()), None, Vec::new()),
                section("其他项目", Some("上次状态".into()), None, Vec::new()),
            ],
        }
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

/// 抽屉在浅色主题下也是深色玻璃，状态色取原型的固定色，不跟浅色 token 走。
pub(super) const TONE_RUNNING: u32 = 0x36dce8ff;
pub(super) const TONE_IDLE: u32 = 0x7b9dbdff;
pub(super) const TONE_AMBER: u32 = 0xf2c773ff;
pub(super) const TONE_RESULT: u32 = 0x64dfd2ff;
pub(super) const TONE_FILE: u32 = 0x80baffff;

pub(super) fn overview_tone(theme: Theme) -> OverviewTone {
    OverviewTone {
        running: TONE_RUNNING,
        idle: TONE_IDLE,
        failed: theme.destructive,
        approval: TONE_AMBER,
        review: TONE_RESULT,
        file: TONE_FILE,
    }
}

pub(super) fn present_overview(
    overview: &RunOverview,
    ego_stops: &[(String, u64, u64)],
    now_ms: u64,
) -> OverviewScreen {
    let current_title = match overview.current_project.as_deref() {
        Some(name) if !name.is_empty() => format!("当前项目 · {name}"),
        _ => "当前项目".into(),
    };
    OverviewScreen {
        sections: vec![
            section(
                "需要你处理",
                None,
                Some(overview.attention.len()),
                overview.attention.iter().map(attention_row).collect(),
            ),
            section(
                current_title,
                Some(format!("{} 个智能体", overview.current_agents.len())),
                None,
                overview
                    .current_agents
                    .iter()
                    .map(|agent| agent_row(agent, ego_stops, now_ms))
                    .collect(),
            ),
            section(
                "其他项目",
                Some("上次状态".into()),
                None,
                overview
                    .other_projects
                    .iter()
                    .map(|project| project_row(project, now_ms))
                    .collect(),
            ),
        ],
    }
}

fn section(
    title: impl Into<String>,
    meta: Option<String>,
    count: Option<usize>,
    rows: Vec<OverviewRow>,
) -> OverviewSection {
    OverviewSection {
        title: title.into(),
        meta,
        count,
        rows,
    }
}

fn attention_row(row: &RunOverviewAttention) -> OverviewRow {
    let (prefix, tone) = match row.kind {
        RunOverviewAttentionKind::Approval => ("待审核", OverviewStatusTone::Approval),
        RunOverviewAttentionKind::Review => ("成果待查看", OverviewStatusTone::Review),
        RunOverviewAttentionKind::File => ("待查看文件", OverviewStatusTone::File),
    };
    let enabled = match row.kind {
        RunOverviewAttentionKind::Approval => {
            row.loaded && !row.run_id.is_empty() && !row.turn_id.is_empty()
        }
        RunOverviewAttentionKind::Review | RunOverviewAttentionKind::File => true,
    };
    let action = match row.kind {
        RunOverviewAttentionKind::Approval => OverviewAction::LocateApproval {
            area_id: row.area_id.clone(),
            node_id: row.node_id,
            run_id: row.run_id.clone(),
            turn_id: row.turn_id.clone(),
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
        RunOverviewAttentionKind::Approval => "定位",
        RunOverviewAttentionKind::Review | RunOverviewAttentionKind::File => "打开",
    };
    OverviewRow {
        primary: format!("{prefix} · {}", row.title),
        secondary: format!("{} · {}", row.agent, row.project),
        detail: None,
        task_line: None,
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
    OverviewRow {
        primary: agent.name.clone(),
        secondary: String::new(),
        detail,
        task_line: agent
            .task
            .as_ref()
            .filter(|task| !task.is_empty())
            .map(|task| format!("任务 · {task}")),
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

fn project_row(project: &RunOverviewProject, now_ms: u64) -> OverviewRow {
    OverviewRow {
        primary: project.name.clone(),
        secondary: project_secondary(project, now_ms),
        detail: None,
        task_line: None,
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
