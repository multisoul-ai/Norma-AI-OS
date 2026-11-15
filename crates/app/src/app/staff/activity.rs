//! Transient task progress. Durable Task.messages remains the conversation authority.
use brain::acp::trace::{TraceItem, TraceKind};
use brain::event::TaskUpdate;
use norma::protocol::{Task, TaskMessage};
use std::time::Instant;

/// 一轮里按到达顺序追加的条目；只追加，已显示条目的相对顺序不变。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TimelineEntry {
    Text(String),
    /// `TaskActivity::tools` 下标。
    Tool(usize),
    /// 建议卡/问答卡的稳定标识（见 `card_key`）；落盘插入 Norma 回复会改变消息下标，所以不能存下标。
    Card(String),
}

#[derive(Default)]
pub(crate) struct TaskActivity {
    pub timeline: Vec<TimelineEntry>,
    pub turn: u64,
    pub queued: usize,
    pub active: bool,
    pub reply: String,
    pub plan: Vec<(String, String)>,
    pub failed: bool,
    pub tools: Vec<ToolActivity>,
    started: Option<Instant>,
    pub elapsed_ms: Option<u64>,
    baseline: usize,
    committed: bool,
}

impl TaskActivity {
    pub fn waiting(&self) -> bool {
        self.active || self.queued > 0
    }

    pub fn apply(&mut self, turn: u64, update: TaskUpdate) {
        match update {
            TaskUpdate::Queued => self.queued += 1,
            TaskUpdate::Started { message_count } if turn > self.turn => {
                *self = Self {
                    turn,
                    queued: self.queued.saturating_sub(1),
                    active: true,
                    started: Some(Instant::now()),
                    baseline: message_count,
                    ..Self::default()
                };
            }
            TaskUpdate::Update(item) if turn == self.turn && self.active => match item.kind {
                TraceKind::Reply if !self.committed => {
                    self.reply.push_str(&item.text);
                    match self.timeline.last_mut() {
                        Some(TimelineEntry::Text(text)) => text.push_str(&item.text),
                        _ => self.timeline.push(TimelineEntry::Text(item.text)),
                    }
                }
                TraceKind::Prompt if !self.committed => {
                    self.reply.clear();
                    self.timeline
                        .retain(|entry| !matches!(entry, TimelineEntry::Text(_)));
                }
                TraceKind::Plan => {
                    self.plan = item
                        .text
                        .lines()
                        .filter_map(|line| {
                            let (status, text) = line.strip_prefix('[')?.split_once("] ")?;
                            Some((status.to_owned(), text.to_owned()))
                        })
                        .collect()
                }
                TraceKind::ToolCall | TraceKind::ToolResult | TraceKind::Permission => {
                    self.update_tool(item)
                }
                _ => {}
            },
            TaskUpdate::Finished { failed } if turn == self.turn => {
                self.active = false;
                self.finish_tools();
                self.failed = failed;
                // Successful completion is sent only after durable TaskChanged.
                if !failed {
                    self.reply.clear();
                }
            }
            _ => {}
        }
    }

    pub fn working(&self) -> bool {
        self.tools
            .iter()
            .any(|tool| tool.status == ToolStatus::Running)
    }

    pub fn duration_ms(&self) -> u64 {
        self.elapsed_ms.unwrap_or_else(|| {
            self.started
                .map_or(0, |start| start.elapsed().as_millis() as u64)
        })
    }

    fn update_tool(&mut self, item: TraceItem) {
        let Some(meta) = item.tool else { return };
        let known = self.tools.len();
        let index = self
            .tools
            .iter()
            .position(|tool| tool.id == meta.id)
            .unwrap_or_else(|| {
                self.tools.push(ToolActivity {
                    id: meta.id.clone(),
                    name: meta.id.clone(),
                    input: String::new(),
                    description: None,
                    output: String::new(),
                    status: ToolStatus::Running,
                    started: Instant::now(),
                    elapsed_ms: None,
                });
                self.timeline.push(TimelineEntry::Tool(known));
                known
            });
        let tool = &mut self.tools[index];
        if let Some(name) = meta.name {
            if item.kind != TraceKind::Permission || tool.name == tool.id {
                tool.name = name;
            }
        }
        if meta.description.is_some() {
            tool.description = meta.description;
        }
        if let Some(input) = meta.input {
            tool.input = input;
        }
        if !item.text.is_empty()
            && matches!(item.kind, TraceKind::ToolResult | TraceKind::Permission)
        {
            tool.output = item.text;
        }
        if tool.status == ToolStatus::Denied {
            return;
        }
        let status = match meta.status.as_deref() {
            Some("denied") => ToolStatus::Denied,
            Some("completed") => ToolStatus::Completed,
            Some("failed") => ToolStatus::Failed,
            _ => tool.status,
        };
        tool.status = status;
        if status != ToolStatus::Running && tool.elapsed_ms.is_none() {
            tool.elapsed_ms = Some(tool.started.elapsed().as_millis() as u64);
        }
    }

    fn finish_tools(&mut self) {
        self.elapsed_ms = Some(self.duration_ms());
        for tool in &mut self.tools {
            if tool.status == ToolStatus::Running {
                tool.status = ToolStatus::Interrupted;
                tool.elapsed_ms = Some(tool.started.elapsed().as_millis() as u64);
            }
        }
    }

    pub fn reconcile(&mut self, task: &Task) {
        // 排队阶段（尚未 Started）的基线是占位值，不能据此认领历史消息。
        if self.turn == 0 {
            return;
        }
        let mut saved = Vec::new();
        for message in task.messages.iter().skip(self.baseline) {
            if let Some(key) = card_key(message) {
                if !self.timeline.contains(&TimelineEntry::Card(key.clone())) {
                    self.timeline.push(TimelineEntry::Card(key));
                }
            } else if let TaskMessage::Norma { text } = message {
                saved.push(text.as_str());
            }
        }
        if !saved.is_empty() && !self.committed {
            // 模型自己用 task reply 写回时没有流式片段，把落盘文字追加到末尾。
            if !self
                .timeline
                .iter()
                .any(|entry| matches!(entry, TimelineEntry::Text(_)))
            {
                self.timeline.push(TimelineEntry::Text(saved.join("\n\n")));
            }
            self.reply.clear();
            self.committed = true;
        }
    }

    /// 本轮的 Norma 回复和卡片由时间线渲染，消息列表须跳过。
    pub fn owns(&self, index: usize, message: &TaskMessage) -> bool {
        self.turn != 0
            && index >= self.baseline
            && match message {
                TaskMessage::Norma { .. } => self.committed,
                _ => card_key(message)
                    .is_some_and(|key| self.timeline.contains(&TimelineEntry::Card(key))),
            }
    }

    pub fn interrupt(&mut self) {
        if self.waiting() {
            self.failed = true;
        }
        self.active = false;
        self.queued = 0;
        self.finish_tools();
    }
}

pub(crate) fn card_key(message: &TaskMessage) -> Option<String> {
    match message {
        TaskMessage::Proposal { proposal_id } => Some(format!("p{proposal_id}")),
        TaskMessage::Question { question } => Some(format!("q{}", question.id)),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolStatus {
    Running,
    Completed,
    Failed,
    Denied,
    Interrupted,
}

pub(crate) struct ToolActivity {
    pub id: String,
    pub name: String,
    pub input: String,
    pub description: Option<String>,
    pub output: String,
    pub status: ToolStatus,
    started: Instant,
    pub elapsed_ms: Option<u64>,
}

#[cfg(test)]
#[path = "../../../tests/unit/staff_activity_tests.rs"]
mod tests;
