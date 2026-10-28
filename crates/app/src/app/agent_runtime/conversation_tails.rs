use agent::{AgentConversationAvailability, AgentConversationTail, AgentConversationTaskState};
use canvas::CanvasNodeKind;
use norma::agent_hook::{
    observe_conversation, ConversationObservation, ConversationObservationKind,
};
use norma::protocol::AgentProfile as WireAgentProfile;
use runtime::AgentTaskState;
use std::collections::HashMap;
use storage::{
    BackgroundConversationTailStore, StoredAgentConversationTail, StoredConversationAvailability,
};

use crate::app::MultisoulApp;

#[derive(Clone, Debug, Default)]
pub(crate) struct ConversationTailMemory {
    by_key: HashMap<(i64, String), AgentConversationTail>,
    seen_fingerprints: HashMap<(i64, String), String>,
}

impl ConversationTailMemory {
    pub(crate) fn tails(&self) -> Vec<AgentConversationTail> {
        let mut tails = self.by_key.values().cloned().collect::<Vec<_>>();
        tails.sort_by(|left, right| {
            right
                .updated_at_unix
                .cmp(&left.updated_at_unix)
                .then_with(|| left.node_id.cmp(&right.node_id))
        });
        tails
    }

    pub(crate) fn apply_observation(
        &mut self,
        definition_id: i64,
        agent_name: String,
        task_state: AgentConversationTaskState,
        observation: ConversationObservation,
    ) -> Option<AgentConversationTail> {
        let session_id = observation.session_id.clone().unwrap_or_default();
        if session_id.is_empty() {
            return None;
        }
        let key = (definition_id, session_id.clone());
        if self
            .seen_fingerprints
            .get(&key)
            .is_some_and(|seen| seen == &observation.event_fingerprint)
        {
            return None;
        }
        self.seen_fingerprints
            .insert(key.clone(), observation.event_fingerprint.clone());

        let mut tail = self.by_key.remove(&key).unwrap_or(AgentConversationTail {
            definition_id,
            node_id: observation.node_id,
            session_id: session_id.clone(),
            agent_name: agent_name.clone(),
            profile: observation.profile,
            task_state,
            last_user_message: None,
            last_agent_message: None,
            updated_at_unix: now_unix(),
            availability: AgentConversationAvailability::Unavailable,
            user_message_truncated: false,
            agent_message_truncated: false,
        });
        tail.node_id = observation.node_id;
        tail.agent_name = agent_name;
        tail.profile = observation.profile;
        tail.task_state = task_state;
        tail.updated_at_unix = now_unix();

        match observation.kind {
            ConversationObservationKind::UserMessage => {
                tail.last_user_message = observation.text;
                tail.user_message_truncated = observation.truncated;
            }
            ConversationObservationKind::CompleteAgentReply => {
                tail.last_agent_message = observation.text;
                tail.agent_message_truncated = observation.truncated;
                tail.availability = AgentConversationAvailability::Available;
            }
            ConversationObservationKind::UnavailableCompleteAgentReply => {
                tail.availability = AgentConversationAvailability::Unavailable;
            }
        }
        self.by_key.insert(key, tail.clone());
        Some(tail)
    }

    pub(crate) fn load_stored(&mut self, tails: Vec<AgentConversationTail>) {
        for tail in tails {
            self.by_key
                .insert((tail.definition_id, tail.session_id.clone()), tail);
        }
    }

    pub(crate) fn remove_node(&mut self, node_id: u64) {
        let keys = self
            .by_key
            .iter()
            .filter(|(_, tail)| tail.node_id == node_id)
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in keys {
            self.by_key.remove(&key);
            self.seen_fingerprints.remove(&key);
        }
    }
}

impl MultisoulApp {
    pub(crate) fn bind_conversation_tails_for_project(&mut self) {
        let stored = self
            .persistence
            .project_store()
            .and_then(|store| store.agent_conversation_tail_store().list().ok())
            .unwrap_or_default();
        self.bind_preloaded_conversation_tails(stored);
    }

    pub(crate) fn bind_preloaded_conversation_tails(
        &mut self,
        stored: Vec<StoredAgentConversationTail>,
    ) {
        self.conversation_tail_memory = ConversationTailMemory::default();
        let Some(project_store) = self.persistence.project_store().cloned() else {
            self.conversation_tail_writer = None;
            return;
        };
        let previous = self.conversation_tail_writer.take();
        self.conversation_tail_writer = Some(BackgroundConversationTailStore::new_after(
            previous,
            project_store.agent_conversation_tail_store(),
        ));
        {
            let tails = stored
                .into_iter()
                .filter_map(from_stored)
                .collect::<Vec<_>>();
            self.conversation_tail_memory.load_stored(tails);
        }
    }

    pub(crate) fn conversation_tails_for_context(&self) -> Vec<AgentConversationTail> {
        let mut tails = self.conversation_tail_memory.tails();
        for tail in &mut tails {
            if let Some(state) = self.runtime.agent_task_state(tail.node_id) {
                tail.task_state = map_task_state(&state);
            }
        }
        tails
    }

    pub(crate) fn ingest_hook_conversation_observation(
        &mut self,
        event: &norma::agent_hook::AgentHookEvent,
    ) {
        let Some(observation) = observe_conversation(event) else {
            return;
        };
        let Some((definition_id, agent_name)) = self.definition_for_node(event.node_id) else {
            return;
        };
        let task_state = self
            .runtime
            .agent_task_state(event.node_id)
            .as_ref()
            .map(map_task_state)
            .unwrap_or(AgentConversationTaskState::Unknown);
        let Some(tail) = self.conversation_tail_memory.apply_observation(
            definition_id,
            agent_name,
            task_state,
            observation,
        ) else {
            return;
        };
        if let Some(writer) = self.conversation_tail_writer.clone() {
            writer.upsert(to_stored(&tail));
        }
    }

    pub(crate) fn clear_conversation_tail_for_node(&mut self, node_id: u64) {
        self.conversation_tail_memory.remove_node(node_id);
        if let Some(writer) = &self.conversation_tail_writer {
            writer.delete_for_node(node_id);
        }
    }

    fn definition_for_node(&self, node_id: u64) -> Option<(i64, String)> {
        let node = self
            .runtime
            .document
            .nodes()
            .iter()
            .find(|node| node.id == node_id)?;
        let CanvasNodeKind::Agent(spec) = &node.kind else {
            return None;
        };
        let definition_id = spec.definition_id?;
        let name = self
            .agent_definitions
            .definitions()
            .iter()
            .find(|definition| definition.id == definition_id)
            .map(|definition| definition.name.clone())
            .unwrap_or_else(|| node.kind.display_title());
        Some((definition_id, name))
    }
}

pub(crate) fn map_task_state(state: &AgentTaskState) -> AgentConversationTaskState {
    match state {
        AgentTaskState::Ready => AgentConversationTaskState::Ready,
        AgentTaskState::Thinking => AgentConversationTaskState::Thinking,
        AgentTaskState::RunningTool { .. } => AgentConversationTaskState::RunningTool,
        AgentTaskState::WaitingApproval { .. } => AgentConversationTaskState::WaitingApproval,
        AgentTaskState::WaitingQuestion => AgentConversationTaskState::WaitingQuestion,
        AgentTaskState::Done => AgentConversationTaskState::Done,
        AgentTaskState::Failed { .. } => AgentConversationTaskState::Failed,
        AgentTaskState::Ended => AgentConversationTaskState::Ended,
    }
}

pub(crate) fn to_stored(tail: &AgentConversationTail) -> StoredAgentConversationTail {
    StoredAgentConversationTail {
        definition_id: tail.definition_id,
        node_id: tail.node_id,
        session_id: tail.session_id.clone(),
        agent_name: tail.agent_name.clone(),
        profile: profile_key(tail.profile).to_string(),
        last_user_message: tail.last_user_message.clone(),
        last_agent_message: tail.last_agent_message.clone(),
        updated_at_unix: tail.updated_at_unix,
        availability: match tail.availability {
            AgentConversationAvailability::Available => StoredConversationAvailability::Available,
            AgentConversationAvailability::Unavailable => {
                StoredConversationAvailability::Unavailable
            }
            AgentConversationAvailability::Stale => StoredConversationAvailability::Stale,
        },
        user_message_truncated: tail.user_message_truncated,
        agent_message_truncated: tail.agent_message_truncated,
    }
}

pub(crate) fn from_stored(tail: StoredAgentConversationTail) -> Option<AgentConversationTail> {
    Some(AgentConversationTail {
        definition_id: tail.definition_id,
        node_id: tail.node_id,
        session_id: tail.session_id,
        agent_name: tail.agent_name,
        profile: parse_profile(&tail.profile)?,
        task_state: AgentConversationTaskState::Unknown,
        last_user_message: tail.last_user_message,
        last_agent_message: tail.last_agent_message,
        updated_at_unix: tail.updated_at_unix,
        availability: match tail.availability {
            StoredConversationAvailability::Available => AgentConversationAvailability::Available,
            StoredConversationAvailability::Unavailable => {
                AgentConversationAvailability::Unavailable
            }
            StoredConversationAvailability::Stale => AgentConversationAvailability::Stale,
        },
        user_message_truncated: tail.user_message_truncated,
        agent_message_truncated: tail.agent_message_truncated,
    })
}

fn parse_profile(value: &str) -> Option<WireAgentProfile> {
    match value {
        "claude" => Some(WireAgentProfile::Claude),
        "codex" => Some(WireAgentProfile::Codex),
        "cursor" => Some(WireAgentProfile::Cursor),
        "grok" => Some(WireAgentProfile::Grok),
        "opencode" => Some(WireAgentProfile::OpenCode),
        "infcode" => Some(WireAgentProfile::InfCode),
        "kimi" => Some(WireAgentProfile::Kimi),
        "zcode" => Some(WireAgentProfile::ZCode),
        "pi" => Some(WireAgentProfile::Pi),
        "hermes" => Some(WireAgentProfile::Hermes),
        _ => None,
    }
}

fn profile_key(profile: WireAgentProfile) -> &'static str {
    match profile {
        WireAgentProfile::Claude => "claude",
        WireAgentProfile::Codex => "codex",
        WireAgentProfile::Cursor => "cursor",
        WireAgentProfile::Grok => "grok",
        WireAgentProfile::OpenCode => "opencode",
        WireAgentProfile::InfCode => "infcode",
        WireAgentProfile::Kimi => "kimi",
        WireAgentProfile::ZCode => "zcode",
        WireAgentProfile::Pi => "pi",
        WireAgentProfile::Hermes => "hermes",
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

pub(crate) type ConversationTailWriter = BackgroundConversationTailStore;
