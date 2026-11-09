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
        if !norma::agent_hook::is_hook_ask_tool(event.profile, event.tool_name.as_deref())
            || event.provider_event != "PreToolUse"
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
                            norma::agent_hook::hook_invocation_id(&event.raw_json)
                                .unwrap_or("")
                                .into(),
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
            || norma::agent_hook::hook_invocation_id(&event.raw_json).is_none()
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
        && norma::agent_hook::hook_invocation_id(&pre.raw_json).is_some()
        && norma::agent_hook::hook_invocation_id(&pre.raw_json)
            == norma::agent_hook::hook_invocation_id(&post.raw_json)
}

fn matches_result(pre: &AgentHookEvent, post: &AgentHookEvent) -> bool {
    if !same_call(pre, post)
        || !norma::agent_hook::is_hook_ask_tool(post.profile, post.tool_name.as_deref())
    {
        return false;
    }
    // Ambiguous labels cannot be answered from the island, but the same call still ends the block.
    if let Ok(question) =
        norma::agent_hook::ClaudeQuestion::from_profile(pre.profile, &pre.raw_json)
    {
        return question.confirms_consumption(&post.raw_json)
            || question.confirms_failure(&post.raw_json);
    }
    let Some(questions) = input_questions(&pre.raw_json) else {
        return false;
    };
    if input_questions(&post.raw_json) != Some(questions) {
        return false;
    }
    match post.provider_event.as_str() {
        "PostToolUse" => response_answers(&post.raw_json),
        "PostToolUseFailure" => failure_error(&post.raw_json),
        _ => false,
    }
}

fn input_questions(raw: &serde_json::Value) -> Option<&serde_json::Value> {
    raw.get("tool_input")
        .or_else(|| raw.get("toolInput"))
        .and_then(|input| input.get("questions"))
        .filter(|questions| !questions.is_null())
}

fn response_answers(raw: &serde_json::Value) -> bool {
    raw.get("tool_response")
        .or_else(|| raw.get("toolResponse"))
        .and_then(|body| body.get("answers"))
        .is_some_and(|answers| answers.is_object())
}

fn failure_error(raw: &serde_json::Value) -> bool {
    match raw.get("error") {
        Some(value) if value.as_str().is_some_and(|text| !text.is_empty()) => true,
        Some(value) => value
            .get("message")
            .and_then(|message| message.as_str())
            .is_some_and(|text| !text.is_empty()),
        None => false,
    }
}
