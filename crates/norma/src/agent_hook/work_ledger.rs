//! Claude / Grok 回合完成判定：工作账本 + 确认窗口。
//!
//! `Stop` 只表示回合结束。后台 Bash / 子 Agent 未结清时保持 Thinking，
//! 确认窗口安静结束后才发出可被下游当成 Done 的 `Stop`。

use super::AgentHookEvent;
use crate::protocol::{AgentProfile, NodeId};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// 空账本的 `Stop` 之后，还要安静这么久才确认完成。
/// 取证里子 Agent 的 task-notification 比空 `Stop` 晚 36ms，800ms 能吃掉这个间隔。
pub const CONFIRMATION_WINDOW_MS: u64 = 800;
/// 账本一直非空时的最长等待。30 分钟 = 30 * 60 * 1000。
pub const BACKGROUND_WAIT_LIMIT_MS: u64 = 30 * 60 * 1000;
/// 进程探针只能延后确认，且同一窗口最多延后这么多次。
pub const MAX_PROCESS_DEFERRALS: u8 = 3;

#[derive(Clone, Debug, PartialEq)]
pub enum CompletionAction {
    /// 按原始事件映射状态。Claude / Grok 以外的 `Stop` 仍立即是 Done。
    Forward(AgentHookEvent),
    /// Claude / Grok 的 `Stop` 尚未确认。调用方把卡片保持在 Thinking，且不发完成副作用。
    KeepThinking { event: AgentHookEvent },
    /// 窗口结束，或后台等待超时。`event` 仍是原来的 `Stop`。
    Confirm {
        event: AgentHookEvent,
        background_unsettled: bool,
    },
    /// 确认窗口开始时查一次本回合新后代。只用于延后，不能单独决定完成。
    ProbeProcesses { node_id: NodeId, since_unix_ms: u64 },
}

#[derive(Debug, Default)]
struct PendingWindow {
    since_ms: u64,
}

#[derive(Debug, Default)]
struct NodeLedger {
    open: HashSet<String>,
    /// tool_use_id → 账本里的 work id。notification 两种 id 都能删。
    aliases: HashMap<String, String>,
    pending: Option<PendingWindow>,
    turn_ended: bool,
    /// 第一次「Stop 且账本非空」的时间。集合仍非空时不重置，变空才清。
    unsettled_since_ms: Option<u64>,
    last_prompt_unix_ms: Option<u64>,
    deferrals: u8,
    stashed_stop: Option<AgentHookEvent>,
}

#[derive(Debug, Default)]
pub struct CompletionJudge {
    nodes: HashMap<NodeId, NodeLedger>,
}

impl CompletionJudge {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe(&mut self, event: &AgentHookEvent, now_ms: u64) -> Vec<CompletionAction> {
        if event.profile == AgentProfile::Grok && !is_grok_main_turn_event(event) {
            return Vec::new();
        }
        if !matches!(event.profile, AgentProfile::Claude | AgentProfile::Grok) {
            return vec![CompletionAction::Forward(event.clone())];
        }
        match event.provider_event.as_str() {
            "SessionEnd" => self.on_session_end(event),
            "StopFailure" => {
                self.nodes.remove(&event.node_id);
                vec![CompletionAction::Forward(event.clone())]
            }
            "Stop" => self.on_stop(event, now_ms),
            "SubagentStop" => self.on_subagent_stop(event),
            "SubagentStart" => {
                self.register_subagent(event);
                self.note_activity(event.node_id, now_ms);
                vec![CompletionAction::Forward(event.clone())]
            }
            "PostToolUse" => {
                self.on_post_tool(event);
                self.note_activity(event.node_id, now_ms);
                vec![CompletionAction::Forward(event.clone())]
            }
            "UserPromptSubmit" => {
                self.on_prompt(event, now_ms);
                self.note_activity(event.node_id, now_ms);
                vec![CompletionAction::Forward(event.clone())]
            }
            _ => {
                self.note_activity(event.node_id, now_ms);
                vec![CompletionAction::Forward(event.clone())]
            }
        }
    }

    pub fn poll(&mut self, now_ms: u64) -> Vec<CompletionAction> {
        let ids: Vec<NodeId> = self.nodes.keys().copied().collect();
        let mut actions = Vec::new();
        for id in ids {
            let Some(node) = self.nodes.get(&id) else {
                continue;
            };
            let window_due = node.pending.as_ref().is_some_and(|pending| {
                now_ms >= pending.since_ms.saturating_add(CONFIRMATION_WINDOW_MS)
            });
            let timeout_due = node.turn_ended
                && !node.open.is_empty()
                && node
                    .unsettled_since_ms
                    .is_some_and(|since| now_ms >= since.saturating_add(BACKGROUND_WAIT_LIMIT_MS));
            if window_due {
                if let Some(action) = self.take_confirm(id, false) {
                    actions.push(action);
                }
            } else if timeout_due {
                if let Some(action) = self.take_confirm(id, true) {
                    actions.push(action);
                }
            }
        }
        actions
    }

    /// 探针晚到且窗口已结束时直接丢掉，不能把已确认的 Done 撤掉。
    pub fn note_process_probe(
        &mut self,
        node_id: NodeId,
        found: bool,
        now_ms: u64,
    ) -> Vec<CompletionAction> {
        let Some(node) = self.nodes.get_mut(&node_id) else {
            return Vec::new();
        };
        if node.pending.is_none() || !found || node.deferrals >= MAX_PROCESS_DEFERRALS {
            return Vec::new();
        }
        node.deferrals = node.deferrals.saturating_add(1);
        if let Some(pending) = node.pending.as_mut() {
            pending.since_ms = now_ms;
        }
        match node.last_prompt_unix_ms {
            Some(since_unix_ms) => vec![CompletionAction::ProbeProcesses {
                node_id,
                since_unix_ms,
            }],
            None => Vec::new(),
        }
    }

    /// 进程退出：清空账本，不补发 Done。
    pub fn abandon(&mut self, node_id: NodeId) {
        self.nodes.remove(&node_id);
    }

    pub fn tracked_nodes(&self) -> Vec<NodeId> {
        self.nodes.keys().copied().collect()
    }

    fn on_stop(&mut self, event: &AgentHookEvent, now_ms: u64) -> Vec<CompletionAction> {
        let decision = {
            let node = self.ledger(event.node_id);
            // stop_hook_active 的最后一次 Stop 也是 true；续跑轮次的活动会作废窗口。
            if let Some(ids) = background_task_ids(&event.raw_json) {
                replace_open(node, ids);
            }
            node.turn_ended = true;
            node.stashed_stop = Some(event.clone());
            if node.open.is_empty() {
                node.unsettled_since_ms = None;
                node.deferrals = 0;
                node.pending = Some(PendingWindow { since_ms: now_ms });
                StopDecision::Window {
                    probe_since: node.last_prompt_unix_ms,
                }
            } else {
                node.pending = None;
                if node.unsettled_since_ms.is_none() {
                    node.unsettled_since_ms = Some(now_ms);
                }
                let since = node.unsettled_since_ms.unwrap_or(now_ms);
                if now_ms >= since.saturating_add(BACKGROUND_WAIT_LIMIT_MS) {
                    StopDecision::Timeout
                } else {
                    StopDecision::Hold
                }
            }
        };
        match decision {
            StopDecision::Hold => vec![CompletionAction::KeepThinking {
                event: event.clone(),
            }],
            StopDecision::Window { probe_since } => {
                let mut actions = vec![CompletionAction::KeepThinking {
                    event: event.clone(),
                }];
                if let Some(since_unix_ms) = probe_since {
                    actions.push(CompletionAction::ProbeProcesses {
                        node_id: event.node_id,
                        since_unix_ms,
                    });
                }
                actions
            }
            StopDecision::Timeout => self.take_confirm(event.node_id, true).into_iter().collect(),
        }
    }

    fn on_subagent_stop(&mut self, event: &AgentHookEvent) -> Vec<CompletionAction> {
        let node = self.ledger(event.node_id);
        if let Some(ids) = background_task_ids(&event.raw_json) {
            replace_open(node, ids);
        } else if let Some(agent_id) = first_string(&event.raw_json, &["agent_id", "agentId"]) {
            remove_work(node, &agent_id);
        }
        node.pending = None;
        node.turn_ended = false;
        vec![CompletionAction::Forward(event.clone())]
    }

    fn on_post_tool(&mut self, event: &AgentHookEvent) {
        let node = self.ledger(event.node_id);
        if is_task_stop(event.tool_name.as_deref()) {
            if let Some(task_id) = task_stop_id(&event.raw_json) {
                remove_work(node, &task_id);
            }
            return;
        }
        if !run_in_background(&event.raw_json) {
            return;
        }
        let tool_use_id = string_at(&event.raw_json, "tool_use_id");
        let work_id = response_work_id(&event.raw_json).or_else(|| tool_use_id.clone());
        let Some(work_id) = work_id else {
            return;
        };
        node.open.insert(work_id.clone());
        if let Some(tool_use_id) = tool_use_id {
            if tool_use_id != work_id {
                node.aliases.insert(tool_use_id, work_id);
            }
        }
    }

    fn on_prompt(&mut self, event: &AgentHookEvent, now_ms: u64) {
        let node = self.ledger(event.node_id);
        node.last_prompt_unix_ms = Some(now_ms);
        if let Some(prompt) = prompt_text(&event.raw_json) {
            for id in settled_notification_ids(&prompt) {
                remove_work(node, &id);
            }
        }
    }

    fn register_subagent(&mut self, event: &AgentHookEvent) {
        let Some(agent_id) = first_string(&event.raw_json, &["agent_id", "agentId"]) else {
            return;
        };
        self.ledger(event.node_id).open.insert(agent_id);
    }

    /// 空账本上的活动作废确认窗口。账本仍非空时保留「回合已结束」，
    /// 若空 Stop 把超时时钟清掉，迟到登记从现在重新武装 30 分钟。
    fn note_activity(&mut self, node_id: NodeId, now_ms: u64) {
        let Some(node) = self.nodes.get_mut(&node_id) else {
            return;
        };
        node.pending = None;
        if node.open.is_empty() {
            node.turn_ended = false;
            return;
        }
        if node.turn_ended && node.unsettled_since_ms.is_none() {
            node.unsettled_since_ms = Some(now_ms);
        }
    }

    /// `claude -p` 在最终空 Stop 后约 34ms 发 SessionEnd，落在 800ms 窗口内。
    /// 账本已空时先确认成功，再转发结束。账本非空则只清空，不补成功。
    fn on_session_end(&mut self, event: &AgentHookEvent) -> Vec<CompletionAction> {
        let quiet = self.nodes.get(&event.node_id).is_some_and(|node| {
            node.pending.is_some() && node.open.is_empty() && node.stashed_stop.is_some()
        });
        let mut actions = Vec::new();
        if quiet {
            if let Some(action) = self.take_confirm(event.node_id, false) {
                actions.push(action);
            }
        }
        self.nodes.remove(&event.node_id);
        actions.push(CompletionAction::Forward(event.clone()));
        actions
    }

    fn ledger(&mut self, node_id: NodeId) -> &mut NodeLedger {
        self.nodes.entry(node_id).or_default()
    }

    fn take_confirm(
        &mut self,
        node_id: NodeId,
        background_unsettled: bool,
    ) -> Option<CompletionAction> {
        let node = self.nodes.get_mut(&node_id)?;
        let event = node.stashed_stop.clone()?;
        node.pending = None;
        node.turn_ended = false;
        node.unsettled_since_ms = None;
        node.open.clear();
        node.aliases.clear();
        node.deferrals = 0;
        node.stashed_stop = None;
        Some(CompletionAction::Confirm {
            event,
            background_unsettled,
        })
    }
}

enum StopDecision {
    Hold,
    Window { probe_since: Option<u64> },
    Timeout,
}

fn replace_open(node: &mut NodeLedger, ids: HashSet<String>) {
    node.open = ids;
    node.aliases.retain(|_, target| node.open.contains(target));
    if node.open.is_empty() {
        node.unsettled_since_ms = None;
    }
}

fn remove_work(node: &mut NodeLedger, id: &str) {
    let canonical = node
        .aliases
        .get(id)
        .cloned()
        .unwrap_or_else(|| id.to_string());
    node.open.remove(&canonical);
    node.open.remove(id);
    node.aliases
        .retain(|alias, target| alias != id && target != &canonical && target != id);
    if node.open.is_empty() {
        node.unsettled_since_ms = None;
    }
}

fn run_in_background(raw: &Value) -> bool {
    raw.get("tool_input")
        .and_then(|input| input.get("run_in_background"))
        .and_then(Value::as_bool)
        == Some(true)
}

fn is_task_stop(tool_name: Option<&str>) -> bool {
    matches!(tool_name, Some("TaskStop" | "KillShell" | "KillBash"))
}

fn task_stop_id(raw: &Value) -> Option<String> {
    raw.get("tool_input")
        .and_then(|input| string_at(input, "task_id"))
        .or_else(|| {
            raw.get("tool_response")
                .and_then(|response| string_at(response, "task_id"))
        })
}

fn response_work_id(raw: &Value) -> Option<String> {
    let response = raw.get("tool_response")?;
    string_at(response, "backgroundTaskId").or_else(|| string_at(response, "agentId"))
}

fn background_task_ids(raw: &Value) -> Option<HashSet<String>> {
    // Grok 只发 camelCase 的 `backgroundTasks`。
    let tasks = raw
        .get("background_tasks")
        .or_else(|| raw.get("backgroundTasks"))?
        .as_array()?;
    Some(
        tasks
            .iter()
            .filter_map(|task| string_at(task, "id"))
            .collect(),
    )
}

/// Grok 子会话的生命周期事件属于子 Agent，不是这张卡片的回合；
/// 会话关闭时附带的 Stop（reason 不是 end_turn）也不是回合完成。
fn is_grok_main_turn_event(event: &AgentHookEvent) -> bool {
    let raw = &event.raw_json;
    let lifecycle = matches!(
        event.provider_event.as_str(),
        "SessionStart" | "SessionEnd" | "UserPromptSubmit" | "Stop" | "StopFailure"
    );
    let in_subagent = raw
        .get("subagentType")
        .is_some_and(|value| !value.is_null());
    let session_close_stop = event.provider_event == "Stop"
        && string_at(raw, "reason").is_some_and(|reason| reason != "end_turn");
    !(session_close_stop || (lifecycle && in_subagent))
}

fn prompt_text(raw: &Value) -> Option<String> {
    first_string(raw, &["prompt", "message", "user_message"])
}

fn settled_notification_ids(prompt: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for block in prompt.split("<task-notification>").skip(1) {
        let Some(status) = tag_text(block, "status") else {
            continue;
        };
        if status.is_empty() || status.eq_ignore_ascii_case("running") {
            continue;
        }
        if let Some(id) = tag_text(block, "task-id") {
            ids.push(id);
        }
        if let Some(id) = tag_text(block, "tool-use-id") {
            ids.push(id);
        }
    }
    ids
}

fn tag_text(block: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = block.find(&open)? + open.len();
    let rest = block.get(start..)?;
    let end = rest.find(&close)?;
    Some(rest[..end].trim().to_string())
}

fn first_string(raw: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| string_at(raw, key))
}

fn string_at(raw: &Value, key: &str) -> Option<String> {
    raw.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
