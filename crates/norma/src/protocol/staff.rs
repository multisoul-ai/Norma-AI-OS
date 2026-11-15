//! 幕僚任务协议。任务与现有 AgentTaskReceipt（单次派发回执）分别建模。
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum StaffOp {
    Create {
        title: String,
    },
    List,
    Message {
        task_id: String,
        text: String,
    },
    Reply {
        task_id: String,
        text: String,
    },
    Phase {
        task_id: String,
        phase: TaskPhase,
    },
    Propose {
        task_id: String,
        agent: String,
        title: String,
        prompt: String,
        why: String,
    },
    Adopt {
        task_id: String,
        proposal_id: u64,
    },
    /// 幕僚自己派活：是否直接派发由宿主按策略决定，不满足条件时降级为提议。
    Assign {
        task_id: String,
        agent: String,
        title: String,
        prompt: String,
        why: String,
    },
    /// 归档或取消归档。模型命令只开放归档，取消归档只由用户在面板里操作。
    Archive {
        task_id: String,
        archived: bool,
    },
}

impl StaffOp {
    pub fn task_id(&self) -> Option<&str> {
        match self {
            Self::Create { .. } | Self::List => None,
            Self::Message { task_id, .. }
            | Self::Reply { task_id, .. }
            | Self::Phase { task_id, .. }
            | Self::Propose { task_id, .. }
            | Self::Adopt { task_id, .. }
            | Self::Assign { task_id, .. }
            | Self::Archive { task_id, .. } => Some(task_id),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TaskPhase {
    Active,
    Merging,
    Done,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TaskMessage {
    Question {
        #[serde(default)]
        question: Box<super::StaffQuestion>,
    },
    User {
        text: String,
    },
    Norma {
        text: String,
    },
    Reopened,
    WorkerFinished {
        node_id: u64,
        dispatch_id: String,
        outcome: WorkerOutcome,
        text: String,
    },
    Proposal {
        proposal_id: u64,
    },
    WorkerStarted {
        proposal_id: u64,
        dispatch_id: String,
    },
    WorkerNeedsAttention {
        node_id: u64,
        dispatch_id: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorkerState {
    Active,
    Failed,
    Interrupted,
    NeedsAttention,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorkerOutcome {
    Succeeded,
    Failed,
    Interrupted,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TaskDisplayStatus {
    NeedsYou,
    Running,
    Waiting,
    Merging,
    Done,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Worker {
    pub agent: String,
    pub engine: String,
    pub role: String,
    pub card_id: Option<u64>,
    pub dispatch_id: String,
    pub engine_session_id: Option<String>,
    pub state: WorkerState,
    #[serde(default)]
    pub needs_input: bool,
    /// Active 表示仍持有卡片；outcome 区分执行中与已取得本轮结果。
    pub outcome: Option<WorkerOutcome>,
    /// 实际打进卡片的 prompt。旧快照没有该字段时为空，重投回退到提议原文。
    #[serde(default)]
    pub launch_prompt: String,
    /// 本轮 UserPromptSubmit 已确认。旧快照缺字段时视为已送达，避免正在执行的 Worker 变成启动中。
    #[serde(default = "prompt_delivered_by_default")]
    pub prompt_delivered: bool,
}

fn prompt_delivered_by_default() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StaffProposal {
    pub id: u64,
    pub agent: String,
    pub title: String,
    pub prompt: String,
    pub why: String,
    /// 用户在采纳时选定的模型；None 表示沿用该 CLI 的默认模型。
    #[serde(default)]
    pub model: Option<String>,
    pub approved: bool,
    #[serde(default)]
    pub ignored: bool,
    pub dispatch_id: Option<String>,
    /// 提出（或最近一次原样重提）时的幕僚轮次。
    #[serde(default)]
    pub round: u64,
    /// 幕僚自己派发（无需用户采用）。
    #[serde(default)]
    pub auto: bool,
    /// 幕僚想直接派发但被降级为提议的原因码（见 `ProposeReason::code`）。
    #[serde(default)]
    pub downgrade: Option<String>,
}

impl Worker {
    pub fn new(agent: &str, engine: &str, role: &str, node_id: u64, dispatch_id: &str) -> Self {
        Self {
            agent: agent.into(),
            engine: engine.into(),
            role: role.into(),
            card_id: Some(node_id),
            dispatch_id: dispatch_id.into(),
            engine_session_id: None,
            state: WorkerState::Active,
            needs_input: false,
            outcome: None,
            launch_prompt: String::new(),
            prompt_delivered: false,
        }
    }

    pub fn chip(&self) -> WorkerChip {
        if self.needs_input
            || matches!(
                self.state,
                WorkerState::Failed | WorkerState::Interrupted | WorkerState::NeedsAttention
            )
            || self.outcome == Some(WorkerOutcome::Unavailable)
        {
            WorkerChip::NeedsYou
        } else if self.outcome.is_some() {
            WorkerChip::Done
        } else if self.prompt_delivered {
            WorkerChip::Running
        } else {
            WorkerChip::Starting
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerChip {
    Starting,
    Running,
    NeedsYou,
    Done,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Task {
    #[serde(default)]
    pub revision: u64,
    pub id: String,
    pub workspace_id: String,
    pub project_id: String,
    pub title: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub acp_session_id: Option<String>,
    pub phase: TaskPhase,
    pub phase_manual: bool,
    pub messages: Vec<TaskMessage>,
    pub workers: Vec<Worker>,
    pub proposals: Vec<StaffProposal>,
    /// 缺字段的旧快照视为未归档。
    #[serde(default)]
    pub archived: bool,
    /// 幕僚已开始的轮次数；每轮开始 +1。
    #[serde(default)]
    pub round: u64,
    /// 幕僚未经用户采用自动派发的次数；用户采用任意一条提议后清零。旧快照缺字段时为 0。
    #[serde(default)]
    pub auto_dispatches: u32,
}

impl Task {
    pub fn new(id: &str, workspace_id: &str, project_id: &str, title: &str) -> Self {
        let now = timestamp();
        Self {
            revision: 0,
            id: id.into(),
            workspace_id: workspace_id.into(),
            project_id: project_id.into(),
            title: title.into(),
            created_at: now,
            updated_at: now,
            acp_session_id: None,
            phase: TaskPhase::Active,
            phase_manual: false,
            messages: Vec::new(),
            workers: Vec::new(),
            proposals: Vec::new(),
            archived: false,
            round: 0,
            auto_dispatches: 0,
        }
    }

    /// 轮末整批替换：本轮提过提议（含原样重提）时，删除更早轮次留下的待采用提议及其消息。
    pub fn supersede_stale_proposals(&mut self) {
        if !self.proposals.iter().any(|p| p.round == self.round) {
            return;
        }
        let round = self.round;
        let stale = |p: &StaffProposal| {
            p.round < round && !p.approved && !p.ignored && p.dispatch_id.is_none()
        };
        let gone: Vec<u64> = self
            .proposals
            .iter()
            .filter(|p| stale(p))
            .map(|p| p.id)
            .collect();
        self.proposals.retain(|p| !stale(p));
        self.messages.retain(
            |m| !matches!(m, TaskMessage::Proposal { proposal_id } if gone.contains(proposal_id)),
        );
    }

    pub fn set_phase(&mut self, phase: TaskPhase, manual: bool) {
        if manual || !self.phase_manual {
            self.phase = phase;
            if phase == TaskPhase::Done {
                self.expire_pending_questions();
            }
            self.phase_manual = manual;
            self.updated_at = timestamp();
        }
    }

    pub fn set_archived(&mut self, archived: bool) {
        self.archived = archived;
        self.updated_at = timestamp();
    }

    /// 归档提示是否要写明任务仍在进行：需要你、执行中、待验收。
    pub fn archive_note_active(&self) -> bool {
        matches!(
            self.display_status(),
            TaskDisplayStatus::NeedsYou | TaskDisplayStatus::Running | TaskDisplayStatus::Merging
        )
    }

    pub fn user_message(&mut self, text: &str) {
        self.archived = false;
        if self.phase == TaskPhase::Done {
            self.phase = TaskPhase::Active;
            self.phase_manual = false;
            self.messages.push(TaskMessage::Reopened);
        }
        self.messages.push(TaskMessage::User { text: text.into() });
        self.updated_at = timestamp();
    }

    pub fn display_status(&self) -> TaskDisplayStatus {
        if self.phase != TaskPhase::Done && self.pending_question_count() > 0 {
            return TaskDisplayStatus::NeedsYou;
        }
        match self.phase {
            TaskPhase::Done => return TaskDisplayStatus::Done,
            TaskPhase::Merging => return TaskDisplayStatus::Merging,
            TaskPhase::Active => {}
        }
        if self
            .proposals
            .iter()
            .any(|proposal| proposal.dispatch_id.is_none() && !proposal.ignored)
            || self.workers.iter().any(|worker| {
                worker.needs_input
                    || worker.state != WorkerState::Active
                    || worker.outcome == Some(WorkerOutcome::Unavailable)
            })
        {
            TaskDisplayStatus::NeedsYou
        } else if self.workers.iter().any(|worker| {
            worker.outcome.is_none()
                && worker.prompt_delivered
                && worker.state == WorkerState::Active
        }) {
            TaskDisplayStatus::Running
        } else {
            TaskDisplayStatus::Waiting
        }
    }

    /// 按派发号找 Worker；同一个 Agent 可以被派多次，不能按名字找。
    pub fn worker_for_dispatch(&self, dispatch_id: &str) -> Option<&Worker> {
        self.workers.iter().find(|w| w.dispatch_id == dispatch_id)
    }

    pub fn finish_worker(
        &mut self,
        node_id: u64,
        dispatch_id: &str,
        outcome: WorkerOutcome,
        text: &str,
    ) -> bool {
        let Some(worker) = self.workers.iter_mut().find(|worker| {
            worker.card_id == Some(node_id)
                && worker.dispatch_id == dispatch_id
                && worker.outcome.is_none()
        }) else {
            return false;
        };
        worker.outcome = Some(outcome);
        worker.needs_input = false;
        worker.state = match outcome {
            WorkerOutcome::Failed => WorkerState::Failed,
            WorkerOutcome::Interrupted => WorkerState::Interrupted,
            _ => WorkerState::Active,
        };
        self.messages.push(TaskMessage::WorkerFinished {
            node_id,
            dispatch_id: dispatch_id.into(),
            outcome,
            text: text.into(),
        });
        self.updated_at = timestamp();
        if self
            .proposals
            .iter()
            .all(|proposal| proposal.dispatch_id.is_some() || proposal.ignored)
            && self
                .workers
                .iter()
                .all(|worker| worker.outcome == Some(WorkerOutcome::Succeeded))
        {
            self.set_phase(TaskPhase::Merging, false);
        }
        true
    }
}

pub fn timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
