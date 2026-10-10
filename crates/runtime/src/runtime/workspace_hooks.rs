//! Keep observed Provider starts distinct from later input, including direct work without an assignment.
use super::Runtime;
use norma::agent_hook::AgentHookEvent;
use std::collections::HashSet;

#[derive(Hash, Eq, PartialEq)]
struct ObservedStart {
    card_instance_id: String,
    provider: String,
    session_id: String,
    turn_id: String,
}

/// Lifetime matches the Runtime, including native /resume and PTY restarts. Never derives a new run for an old Hook.
#[derive(Default)]
pub(super) struct WorkspaceHookHistory {
    starts: HashSet<ObservedStart>,
}

impl Runtime {
    /// Reject repeated known starts before they mutate latest turn, input acknowledgements or task state.
    pub fn observe_mobile_hook(&mut self, event: &AgentHookEvent) -> bool {
        let key = self.workspace_hook_start_key(event);
        if key
            .as_ref()
            .is_some_and(|key| self.workspace_hooks.starts.contains(key))
        {
            return false;
        }
        if !self.observe_mobile_hook_once(event) {
            return false;
        }
        if let Some(key) = key {
            self.workspace_hooks.starts.insert(key);
        }
        true
    }

    fn workspace_hook_start_key(&self, event: &AgentHookEvent) -> Option<ObservedStart> {
        if event.provider_event != "UserPromptSubmit" {
            return None;
        }
        let (Some(session_id), Some(turn_id)) = (&event.session_id, &event.turn_id) else {
            return None;
        };
        if session_id.is_empty() || turn_id.is_empty() {
            return None;
        }
        Some(ObservedStart {
            card_instance_id: self.document.node(event.node_id)?.card_instance_id.clone(),
            provider: format!("{:?}", event.profile),
            session_id: session_id.clone(),
            turn_id: turn_id.clone(),
        })
    }
}
