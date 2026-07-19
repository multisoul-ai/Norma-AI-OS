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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentTaskCancellation {
    Cancelled,
    CancelRequested,
    NotFound,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentCoordinatedTask {
    pub task_id: u64,
    pub definition_id: i64,
    pub agent: String,
    pub task: String,
    pub state: &'static str,
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
    cancel_requested: bool,
}

#[derive(Clone, Debug)]
struct CoordinatorAgent {
    roster: AgentRosterEntry,
    active: Option<QueuedAgentTask>,
    queue: VecDeque<QueuedAgentTask>,
}

#[derive(Clone, Debug)]
pub struct AgentTaskCoordinator {
    agents: Vec<CoordinatorAgent>,
    lookup: HashMap<String, usize>,
    completed: VecDeque<AgentCoordinatedTask>,
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
                active: None,
                queue: VecDeque::new(),
            });
        }
        Ok(Self {
            agents,
            lookup,
            completed: VecDeque::new(),
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
                        cancel_requested: false,
                    });
                    AgentDispatchState::Queued
                }
                (AgentTrackingMode::Managed, false) => {
                    agent.roster.busy = true;
                    agent.active = Some(QueuedAgentTask {
                        task_id,
                        task: assignment.task.clone(),
                        cancel_requested: false,
                    });
                    AgentDispatchState::SubmittedManaged
                }
                (AgentTrackingMode::Untracked, _) => {
                    agent.roster.busy = true;
                    agent.active = Some(QueuedAgentTask {
                        task_id,
                        task: assignment.task.clone(),
                        cancel_requested: false,
                    });
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
        let completed = std::mem::take(&mut self.completed);
        let mut previous = self
            .agents
            .drain(..)
            .map(|agent| {
                (
                    agent.roster.definition_id,
                    (agent.active, agent.queue, agent.roster.busy),
                )
            })
            .collect::<HashMap<_, _>>();
        let mut replacement = Self::new(roster)?;
        replacement.next_task_id = next_task_id;
        replacement.completed = completed;
        for agent in &mut replacement.agents {
            if let Some((active, queue, was_busy)) = previous.remove(&agent.roster.definition_id) {
                agent.active = active;
                agent.queue = queue;
                agent.roster.busy |= was_busy;
            }
        }
        *self = replacement;
        Ok(())
    }

    pub fn finish_current(&mut self, definition_id: i64) -> Option<AgentDispatchReceipt> {
        self.finish_current_with_state(definition_id, "succeeded")
    }

    pub fn finish_current_with_state(
        &mut self,
        definition_id: i64,
        terminal_state: &'static str,
    ) -> Option<AgentDispatchReceipt> {
        let agent = self
            .agents
            .iter_mut()
            .find(|agent| agent.roster.definition_id == definition_id)?;
        let node_id = agent.roster.active_node_id?;
        if let Some(completed) = agent.active.take() {
            self.completed.push_back(AgentCoordinatedTask {
                task_id: completed.task_id,
                definition_id,
                agent: agent.roster.name.clone(),
                task: completed.task,
                state: terminal_state,
            });
            while self.completed.len() > 100 {
                self.completed.pop_front();
            }
        }
        if let Some(next) = agent.queue.pop_front() {
            agent.roster.busy = true;
            agent.active = Some(next.clone());
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
        if agent
            .active
            .as_ref()
            .is_some_and(|active| task_ids.is_empty() || task_ids.contains(&active.task_id))
        {
            agent.active = None;
        }
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
        agent.active = None;
        agent.roster.busy = false;
        Some((was_busy, cancelled_queued))
    }

    pub fn cancel_task(&mut self, task_id: u64) -> AgentTaskCancellation {
        for agent in &mut self.agents {
            if let Some(index) = agent
                .queue
                .iter()
                .position(|queued| queued.task_id == task_id)
            {
                let cancelled = agent
                    .queue
                    .remove(index)
                    .expect("queued task index was found immediately above");
                self.completed.push_back(AgentCoordinatedTask {
                    task_id: cancelled.task_id,
                    definition_id: agent.roster.definition_id,
                    agent: agent.roster.name.clone(),
                    task: cancelled.task,
                    state: "cancelled",
                });
                while self.completed.len() > 100 {
                    self.completed.pop_front();
                }
                return AgentTaskCancellation::Cancelled;
            }
            if let Some(active) = agent
                .active
                .as_mut()
                .filter(|active| active.task_id == task_id)
            {
                active.cancel_requested = true;
                return AgentTaskCancellation::CancelRequested;
            }
        }
        AgentTaskCancellation::NotFound
    }

    pub fn cancel_unsubmitted_task(
        &mut self,
        task_id: u64,
    ) -> (AgentTaskCancellation, Option<AgentDispatchReceipt>) {
        let definition_id = self.agents.iter().find_map(|agent| {
            agent
                .active
                .as_ref()
                .filter(|active| active.task_id == task_id)
                .map(|_| agent.roster.definition_id)
        });
        match definition_id {
            Some(definition_id) => (
                AgentTaskCancellation::Cancelled,
                self.finish_current_with_state(definition_id, "cancelled"),
            ),
            None => (AgentTaskCancellation::NotFound, None),
        }
    }

    pub fn tasks(&self) -> Vec<AgentCoordinatedTask> {
        let mut tasks = self.completed.iter().cloned().collect::<Vec<_>>();
        for agent in &self.agents {
            if let Some(active) = &agent.active {
                tasks.push(AgentCoordinatedTask {
                    task_id: active.task_id,
                    definition_id: agent.roster.definition_id,
                    agent: agent.roster.name.clone(),
                    task: active.task.clone(),
                    state: if active.cancel_requested {
                        "cancel_requested"
                    } else {
                        "running"
                    },
                });
            }
            tasks.extend(agent.queue.iter().map(|queued| AgentCoordinatedTask {
                task_id: queued.task_id,
                definition_id: agent.roster.definition_id,
                agent: agent.roster.name.clone(),
                task: queued.task.clone(),
                state: "queued",
            }));
        }
        tasks.sort_by_key(|task| task.task_id);
        tasks
    }

    pub fn task_id_for(&self, definition_id: i64, task: &str) -> Option<u64> {
        self.tasks()
            .into_iter()
            .rev()
            .find(|candidate| candidate.definition_id == definition_id && candidate.task == task)
            .map(|candidate| candidate.task_id)
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
