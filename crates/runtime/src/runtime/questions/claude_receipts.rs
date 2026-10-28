//! Ingress workers may enqueue a tool result before the question mailbox is drained.
use super::*;
use norma::agent_hook::{AgentHookEvent, HookQuestion, QUESTION_TIMEOUT};
use std::time::Instant;

impl Runtime {
    pub(super) fn remember_early_native_result(&mut self, event: &AgentHookEvent) {
        if self.native_question_process_id(event.node_id) != event.control_run_id.as_deref()
            || event.control_run_id.is_none()
            || self.questions.entries.iter().any(|entry| {
                entry.hook.as_ref().is_some_and(|hook| {
                    hook.request().event.node_id == event.node_id
                        && hook.request().event.turn_id == event.turn_id
                        && (hook.adapter().confirms_consumption(&event.raw_json)
                            || hook.adapter().confirms_failure(&event.raw_json))
                })
            })
        {
            return;
        }
        let now = Instant::now();
        let buffered = &mut self.questions.early_native_results;
        buffered.retain(|(at, _)| now.duration_since(*at) < QUESTION_TIMEOUT);
        if buffered.iter().any(|(_, previous)| {
            previous.node_id == event.node_id
                && previous.control_run_id == event.control_run_id
                && previous.raw_json == event.raw_json
        }) {
            return;
        }
        // Same bound as the live question registry; old results never generate an answer.
        if buffered.len() >= 256 {
            buffered.pop_front();
        }
        buffered.push_back((now, event.clone()));
    }

    pub(super) fn reconcile_early_native_results(&mut self, hook: &HookQuestion) {
        let matches = |event: &AgentHookEvent| {
            event.node_id == hook.request().event.node_id
                && event.control_run_id.as_deref() == Some(hook.request().run_id.as_str())
                && hook
                    .request()
                    .event
                    .turn_id
                    .as_ref()
                    .is_none_or(|turn| event.turn_id.as_ref() == Some(turn))
                && (hook.adapter().confirms_consumption(&event.raw_json)
                    || hook.adapter().confirms_failure(&event.raw_json))
        };
        if let Some(index) = self
            .questions
            .early_native_results
            .iter()
            .position(|(_, event)| matches(event))
        {
            let (_, event) = self
                .questions
                .early_native_results
                .remove(index)
                .expect("matched receipt");
            self.observe_native_question_result(&event);
        }
    }
}
