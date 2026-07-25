use crate::AgentName;
use norma::protocol::AgentProfile;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

mod delegation;
mod lifecycle;

const MAX_UNFINISHED_DESCENDANTS: usize = 32;
static RUNTIME_INSTANCE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentTrackingMode {
    Managed,
    Untracked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentRosterEntry {
    pub definition_id: i64,
    pub name: String,
    pub speech_alias: Option<String>,
    pub profile: AgentProfile,
    pub enabled: bool,
    pub active_node_id: Option<u64>,
    pub tracking: AgentTrackingMode,
    pub busy: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentAssignment {
    pub target: String,
    pub task: String,
}

impl AgentAssignment {
    pub fn new(target: impl Into<String>, task: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            task: task.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentDispatchState {
    SubmittedManaged,
    Queued,
    SubmittedUntracked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentTaskCancellation {
    Cancelled,
    CancelRequested,
    NotFound,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AgentDispatchSource {
    pub definition_id: Option<i64>,
    pub node_id: Option<u64>,
}

impl AgentDispatchSource {
    pub const fn norma() -> Self {
        Self {
            definition_id: None,
            node_id: None,
        }
    }

    pub const fn agent(definition_id: i64, node_id: u64) -> Self {
        Self {
            definition_id: Some(definition_id),
            node_id: Some(node_id),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentCoordinatedTask {
    pub task_id: u64,
    pub task_ref: String,
    pub root_task_ref: String,
    pub parent_task_ref: Option<String>,
    pub source_definition_id: Option<i64>,
    pub source_node_id: Option<u64>,
    pub definition_id: i64,
    pub target_node_id: u64,
    pub agent: String,
    pub task: String,
    pub state: &'static str,
    pub tracking: AgentTrackingMode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentDispatchReceipt {
    pub task_id: u64,
    pub task_ref: String,
    pub root_task_ref: String,
    pub parent_task_ref: Option<String>,
    pub source_definition_id: Option<i64>,
    pub source_node_id: Option<u64>,
    pub definition_id: i64,
    pub name: String,
    pub node_id: u64,
    pub task: String,
    pub state: AgentDispatchState,
}

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum AgentCoordinationError {
    #[error("Agent 名称无效：{0}")]
    InvalidName(String),
    #[error("Agent 名称或语音别名冲突：{0}")]
    DuplicateName(String),
    #[error("未找到 Agent：{0}")]
    UnknownName(String),
    #[error("Agent 已禁用：{0}")]
    Disabled(String),
    #[error("Agent 尚未激活卡片：{0}")]
    Inactive(String),
    #[error("任务正文不能为空：{0}")]
    EmptyTask(String),
    #[error("Agent 无 Hook 跟踪且可能仍忙碌，请明确说“仍然发送给 {0}”")]
    UntrackedRequiresForce(String),
    #[error("来源 Agent 当前没有可委派的运行任务")]
    SourceAgentUnavailable,
    #[error("Agent 不得向自己派发任务：definition_id={definition_id}")]
    SelfDispatchForbidden { definition_id: i64 },
    #[error("目标 Agent 位于当前活跃祖先链：definition_id={target_definition_id}")]
    DelegationCycle { target_definition_id: i64 },
    #[error("根任务 {root_task_ref} 已达到 {limit} 个未完成后代")]
    DelegationBudgetExceeded { root_task_ref: String, limit: usize },
}

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum AgentTaskReferenceError {
    #[error("任务引用格式无效")]
    Invalid,
    #[error("任务引用属于已过期运行实例")]
    RuntimeExpired,
    #[error("未找到任务")]
    NotFound,
}

#[derive(Clone, Debug)]
struct QueuedAgentTask {
    task_id: u64,
    task_ref: String,
    root_task_ref: String,
    parent_task_ref: Option<String>,
    source_definition_id: Option<i64>,
    source_node_id: Option<u64>,
    target_node_id: u64,
    task: String,
    cancel_requested: bool,
}

#[derive(Clone, Debug)]
struct UnfinishedTaskLineage {
    parent_task_ref: Option<String>,
    source_definition_id: Option<i64>,
    target_definition_id: i64,
}

#[derive(Clone, Debug)]
struct CoordinatorAgent {
    roster: AgentRosterEntry,
    active: Option<QueuedAgentTask>,
    queue: VecDeque<QueuedAgentTask>,
}

#[derive(Clone, Debug)]
pub struct AgentTaskCoordinator {
    runtime_instance_id: String,
    agents: Vec<CoordinatorAgent>,
    lookup: HashMap<String, usize>,
    completed: VecDeque<AgentCoordinatedTask>,
    unfinished_tasks: HashMap<String, UnfinishedTaskLineage>,
    unfinished_descendants: HashMap<String, usize>,
    next_task_id: u64,
}

pub fn render_agent_roster(roster: &[AgentRosterEntry]) -> String {
    let mut lines = vec!["<agent-roster>".to_string()];
    for entry in roster {
        let alias = effective_speech_alias(entry).unwrap_or("-");
        lines.push(format!(
            "name={} alias={} profile={:?} enabled={} active={} tracking={:?} busy={}",
            entry.name,
            alias,
            entry.profile,
            entry.enabled,
            entry.active_node_id.is_some(),
            entry.tracking,
            entry.busy
        ));
    }
    lines.push("</agent-roster>".to_string());
    lines.join("\n")
}

impl AgentTaskCoordinator {
    pub fn new(roster: Vec<AgentRosterEntry>) -> Result<Self, AgentCoordinationError> {
        Self::new_with_runtime_id(new_runtime_instance_id(), roster)
    }

    pub fn new_with_runtime_id(
        runtime_instance_id: impl Into<String>,
        roster: Vec<AgentRosterEntry>,
    ) -> Result<Self, AgentCoordinationError> {
        let mut lookup = HashMap::new();
        let mut agents = Vec::with_capacity(roster.len());
        for entry in roster {
            let name = AgentName::parse(&entry.name)
                .map_err(|_| AgentCoordinationError::InvalidName(entry.name.clone()))?;
            let index = agents.len();
            insert_lookup_key(&mut lookup, name.match_key(), index, &entry.name)?;
            if let Some(alias) = effective_speech_alias(&entry) {
                let alias_name = AgentName::parse(alias)
                    .map_err(|_| AgentCoordinationError::InvalidName(alias.to_string()))?;
                insert_lookup_key(&mut lookup, alias_name.match_key(), index, alias)?;
            }
            agents.push(CoordinatorAgent {
                roster: entry,
                active: None,
                queue: VecDeque::new(),
            });
        }
        Ok(Self {
            runtime_instance_id: runtime_instance_id.into(),
            agents,
            lookup,
            completed: VecDeque::new(),
            unfinished_tasks: HashMap::new(),
            unfinished_descendants: HashMap::new(),
            next_task_id: 1,
        })
    }

    pub fn dispatch(
        &mut self,
        assignments: Vec<AgentAssignment>,
        force_untracked: bool,
    ) -> Result<Vec<AgentDispatchReceipt>, AgentCoordinationError> {
        self.dispatch_from(assignments, force_untracked, AgentDispatchSource::norma())
    }

    fn preflight(
        &self,
        assignment: &AgentAssignment,
        force_untracked: bool,
    ) -> Result<usize, AgentCoordinationError> {
        if assignment.task.trim().is_empty() {
            return Err(AgentCoordinationError::EmptyTask(assignment.target.clone()));
        }
        let target = AgentName::parse(trim_address_punctuation(&assignment.target))
            .map_err(|_| AgentCoordinationError::UnknownName(assignment.target.clone()))?;
        let index = self
            .lookup
            .get(target.match_key())
            .copied()
            .ok_or_else(|| AgentCoordinationError::UnknownName(assignment.target.clone()))?;
        let agent = &self.agents[index].roster;
        if !agent.enabled {
            return Err(AgentCoordinationError::Disabled(agent.name.clone()));
        }
        if agent.active_node_id.is_none() {
            return Err(AgentCoordinationError::Inactive(agent.name.clone()));
        }
        if agent.tracking == AgentTrackingMode::Untracked && agent.busy && !force_untracked {
            return Err(AgentCoordinationError::UntrackedRequiresForce(
                agent.name.clone(),
            ));
        }
        Ok(index)
    }
}

fn insert_lookup_key(
    lookup: &mut HashMap<String, usize>,
    key: &str,
    index: usize,
    display: &str,
) -> Result<(), AgentCoordinationError> {
    if lookup.insert(key.to_string(), index).is_some() {
        return Err(AgentCoordinationError::DuplicateName(display.to_string()));
    }
    Ok(())
}

fn effective_speech_alias(entry: &AgentRosterEntry) -> Option<&str> {
    entry.speech_alias.as_deref()
}

fn trim_address_punctuation(value: &str) -> &str {
    value.trim().trim_matches(|character: char| {
        matches!(character, ',' | '，' | ':' | '：' | '。' | '!' | '！')
    })
}

fn mark_task_terminal_maps(
    _unfinished_tasks: &mut HashMap<String, UnfinishedTaskLineage>,
    unfinished_descendants: &mut HashMap<String, usize>,
    task: &QueuedAgentTask,
) {
    if task.task_ref == task.root_task_ref {
        return;
    }
    if let Some(count) = unfinished_descendants.get_mut(&task.root_task_ref) {
        *count = count.saturating_sub(1);
        if *count == 0 {
            unfinished_descendants.remove(&task.root_task_ref);
        }
    }
}

fn new_runtime_instance_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let sequence = RUNTIME_INSTANCE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{timestamp:x}-{sequence:x}")
}
