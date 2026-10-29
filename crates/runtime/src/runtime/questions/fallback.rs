//! Unsupported questions still own the native input. This marker grants no answer capability.
use super::*;
use norma::agent_hook::AgentHookEvent;
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct FallbackQuestions {
    pending: HashMap<NodeId, Vec<AgentHookEvent>>,
    // Saturation cannot make untracked native input writable. A session boundary clears it.
    saturated: HashSet<NodeId>,
}
impl FallbackQuestions {
    pub(super) fn blocks(&self, node: NodeId) -> bool {
        self.saturated.contains(&node)
            || self
                .pending
                .get(&node)
                .is_some_and(|items| !items.is_empty())
    }
    pub(super) fn remove_node(&mut self, node: NodeId) -> bool {
        let pending = self.pending.remove(&node).is_some();
        self.saturated.remove(&node) || pending
    }
}

impl Runtime {
    pub(in crate::runtime) fn retain_claude_question_blocker(&mut self, event: &AgentHookEvent) {
        if event.profile != norma::protocol::AgentProfile::Claude
            || event.provider_event != "PreToolUse"
            || event.tool_name.as_deref() != Some("AskUserQuestion")
            || event.control_run_id.is_none()
            || self.native_question_process_id(event.node_id) != event.control_run_id.as_deref()
            || self.agent_session_id(event.node_id) != event.session_id
            || self.questions.ended_sessions.get(&event.node_id) == event.session_id.as_ref()
            || self
                .questions
                .early_native_results
                .iter()
                .any(|(_, post)| matches_result(event, post))
            || self.questions.entries.iter().any(|entry| {
                let target = &entry.snapshot.question.target;
                target.node_id == event.node_id
                    && Some(&target.session_id) == event.session_id.as_ref()
                    && target.request_id
                        == norma::protocol::QuestionRequestId::String(
                            event.raw_json["tool_use_id"].as_str().unwrap_or("").into(),
                        )
                    && matches!(
                        entry.snapshot.state,
                        QuestionState::Answered
                            | QuestionState::AnsweredElsewhere
                            | QuestionState::Cancelled
                    )
            })
        {
            return;
        }
        let pending = self
            .questions
            .fallback
            .pending
            .entry(event.node_id)
            .or_default();
        if pending.iter().any(|pre| same_call(pre, event)) {
            return;
        }
        if pending.len() >= 32
            || event.raw_json.to_string().len() > 65536
            || event.raw_json["tool_use_id"]
                .as_str()
                .is_none_or(str::is_empty)
        {
            self.questions.fallback.saturated.insert(event.node_id);
        } else {
            pending.push(event.clone());
        }
        self.questions
            .provider_states
            .entry(event.node_id)
            .or_insert_with(|| {
                self.events
                    .agent_task_state(event.node_id)
                    .unwrap_or(crate::AgentTaskState::Ready)
            });
        self.refresh_question_projection(event.node_id);
    }

    pub(super) fn release_claude_question_blocker(&mut self, event: &AgentHookEvent) -> bool {
        if self.native_question_process_id(event.node_id) != event.control_run_id.as_deref()
            || event.control_run_id.is_none()
        {
            return false;
        }
        let Some(pending) = self.questions.fallback.pending.get_mut(&event.node_id) else {
            return false;
        };
        let before = pending.len();
        pending.retain(|pre| !matches_result(pre, event));
        let changed = before != pending.len();
        if changed {
            self.refresh_question_projection(event.node_id);
        }
        changed
    }
}

fn same_call(pre: &AgentHookEvent, post: &AgentHookEvent) -> bool {
    pre.node_id == post.node_id
        && pre.control_run_id == post.control_run_id
        && pre.session_id == post.session_id
        && pre
            .turn_id
            .as_ref()
            .is_none_or(|turn| post.turn_id.as_ref() == Some(turn))
        && pre.raw_json["tool_use_id"].as_str().is_some()
        && pre.raw_json["tool_use_id"] == post.raw_json["tool_use_id"]
}

fn matches_result(pre: &AgentHookEvent, post: &AgentHookEvent) -> bool {
    same_call(pre, post)
        && post.profile == norma::protocol::AgentProfile::Claude
        && post.tool_name.as_deref() == Some("AskUserQuestion")
        && !pre.raw_json["tool_input"]["questions"].is_null()
        && pre.raw_json["tool_input"]["questions"] == post.raw_json["tool_input"]["questions"]
        && match post.provider_event.as_str() {
            "PostToolUse" => post.raw_json["tool_response"]["answers"].is_object(),
            "PostToolUseFailure" => post.raw_json["error"].is_string(),
            _ => false,
        }
}
