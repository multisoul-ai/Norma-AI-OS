use crate::AgentName;
use norma::protocol::AgentProfile;
use std::collections::{HashMap, VecDeque};
use thiserror::Error;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentDispatchReceipt {
    pub task_id: u64,
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
}

#[derive(Clone, Debug)]
struct QueuedAgentTask {
    task_id: u64,
    task: String,
}

#[derive(Clone, Debug)]
struct CoordinatorAgent {
    roster: AgentRosterEntry,
    queue: VecDeque<QueuedAgentTask>,
}

#[derive(Clone, Debug)]
pub struct AgentTaskCoordinator {
    agents: Vec<CoordinatorAgent>,
    lookup: HashMap<String, usize>,
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
                queue: VecDeque::new(),
            });
        }
        Ok(Self {
            agents,
            lookup,
            next_task_id: 1,
        })
    }

    pub fn dispatch(
        &mut self,
        assignments: Vec<AgentAssignment>,
        force_untracked: bool,
    ) -> Result<Vec<AgentDispatchReceipt>, AgentCoordinationError> {
        let resolved = assignments
            .iter()
            .map(|assignment| self.preflight(assignment, force_untracked))
            .collect::<Result<Vec<_>, _>>()?;

        let mut receipts = Vec::with_capacity(assignments.len());
        for (assignment, index) in assignments.into_iter().zip(resolved) {
            let task_id = self.next_task_id;
            self.next_task_id = self.next_task_id.saturating_add(1);
            let agent = &mut self.agents[index];
            let node_id = agent
                .roster
                .active_node_id
                .expect("preflight guarantees an active node");
            let state = match (agent.roster.tracking, agent.roster.busy) {
                (AgentTrackingMode::Managed, true) => {
                    agent.queue.push_back(QueuedAgentTask {
                        task_id,
                        task: assignment.task.clone(),
                    });
                    AgentDispatchState::Queued
                }
                (AgentTrackingMode::Managed, false) => {
                    agent.roster.busy = true;
                    AgentDispatchState::SubmittedManaged
                }
                (AgentTrackingMode::Untracked, _) => {
                    agent.roster.busy = true;
                    AgentDispatchState::SubmittedUntracked
                }
            };
            receipts.push(AgentDispatchReceipt {
                task_id,
                definition_id: agent.roster.definition_id,
                name: agent.roster.name.clone(),
                node_id,
                task: assignment.task,
                state,
            });
        }
        Ok(receipts)
    }

    pub fn sync_roster(
        &mut self,
        roster: Vec<AgentRosterEntry>,
    ) -> Result<(), AgentCoordinationError> {
        let next_task_id = self.next_task_id;
        let mut previous = self
            .agents
            .drain(..)
            .map(|agent| (agent.roster.definition_id, (agent.queue, agent.roster.busy)))
            .collect::<HashMap<_, _>>();
        let mut replacement = Self::new(roster)?;
        replacement.next_task_id = next_task_id;
        for agent in &mut replacement.agents {
            if let Some((queue, was_busy)) = previous.remove(&agent.roster.definition_id) {
                agent.queue = queue;
                agent.roster.busy |= was_busy;
            }
        }
        *self = replacement;
        Ok(())
    }

    pub fn finish_current(&mut self, definition_id: i64) -> Option<AgentDispatchReceipt> {
        let agent = self
            .agents
            .iter_mut()
            .find(|agent| agent.roster.definition_id == definition_id)?;
        let node_id = agent.roster.active_node_id?;
        if let Some(next) = agent.queue.pop_front() {
            agent.roster.busy = true;
            Some(AgentDispatchReceipt {
                task_id: next.task_id,
                definition_id,
                name: agent.roster.name.clone(),
                node_id,
                task: next.task,
                state: AgentDispatchState::SubmittedManaged,
            })
        } else {
            agent.roster.busy = false;
            None
        }
    }

    pub fn reject_dispatch(&mut self, definition_id: i64, task_ids: &[u64]) -> bool {
        let Some(agent) = self
            .agents
            .iter_mut()
            .find(|agent| agent.roster.definition_id == definition_id)
        else {
            return false;
        };
        agent
            .queue
            .retain(|queued| !task_ids.contains(&queued.task_id));
        agent.roster.busy = false;
        true
    }

    pub fn queued_task_count(&self, definition_id: i64) -> usize {
        self.agents
            .iter()
            .find(|agent| agent.roster.definition_id == definition_id)
            .map(|agent| agent.queue.len())
            .unwrap_or(0)
    }

    pub fn is_busy(&self, definition_id: i64) -> bool {
        self.agents
            .iter()
            .find(|agent| agent.roster.definition_id == definition_id)
            .map(|agent| agent.roster.busy)
            .unwrap_or(false)
    }

    pub fn cancel_all(&mut self, definition_id: i64) -> Option<(bool, usize)> {
        let agent = self
            .agents
            .iter_mut()
            .find(|agent| agent.roster.definition_id == definition_id)?;
        let was_busy = agent.roster.busy;
        let cancelled_queued = agent.queue.len();
        agent.queue.clear();
        agent.roster.busy = false;
        Some((was_busy, cancelled_queued))
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
