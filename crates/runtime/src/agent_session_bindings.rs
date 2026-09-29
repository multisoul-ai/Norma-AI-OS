//! AgentCard 与 Provider session 的一对一运行时绑定。

use canvas::{AgentProfile, CanvasNodeKind, Document, NodeId};
use std::collections::HashMap;
use storage::{NodeSession, NodeSessionKind};

use crate::Runtime;

#[derive(Clone, Debug, Eq, PartialEq)]
struct BoundAgentSession {
    profile: AgentProfile,
    session_id: String,
}

#[derive(Debug, Default)]
pub(crate) struct AgentSessionBindings {
    by_node: HashMap<NodeId, BoundAgentSession>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AgentSessionHydrationReport {
    pub bound_count: usize,
    pub conflicting_node_ids: Vec<NodeId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CandidateBinding {
    node_id: NodeId,
    profile: AgentProfile,
    session_id: String,
}

impl AgentSessionBindings {
    pub(crate) fn hydrate(
        &mut self,
        document: &Document,
        sessions: &[NodeSession],
    ) -> AgentSessionHydrationReport {
        self.by_node.clear();
        let candidates = sessions
            .iter()
            .filter(|session| session.kind == NodeSessionKind::Agent)
            .filter_map(|session| {
                let session_id = session
                    .session_id
                    .as_deref()
                    .filter(|session_id| !session_id.is_empty())?;
                let CanvasNodeKind::Agent(spec) = &document.node(session.node_id)?.kind else {
                    return None;
                };
                Some(CandidateBinding {
                    node_id: session.node_id,
                    profile: spec.profile,
                    session_id: session_id.to_string(),
                })
            })
            .collect::<Vec<_>>();

        let mut report = AgentSessionHydrationReport::default();
        for candidate in &candidates {
            let owner_count = candidates
                .iter()
                .filter(|other| {
                    other.profile == candidate.profile && other.session_id == candidate.session_id
                })
                .count();
            if owner_count == 1 {
                self.by_node.insert(
                    candidate.node_id,
                    BoundAgentSession {
                        profile: candidate.profile,
                        session_id: candidate.session_id.clone(),
                    },
                );
                report.bound_count += 1;
            } else {
                report.conflicting_node_ids.push(candidate.node_id);
            }
        }
        report.conflicting_node_ids.sort_unstable();
        report.conflicting_node_ids.dedup();
        report
    }

    pub(crate) fn bind_if_unclaimed(
        &mut self,
        node_id: NodeId,
        profile: AgentProfile,
        session_id: &str,
    ) -> bool {
        if session_id.is_empty()
            || self.by_node.iter().any(|(owner, binding)| {
                *owner != node_id && binding.profile == profile && binding.session_id == session_id
            })
        {
            return false;
        }
        match self.by_node.get(&node_id) {
            Some(current)
                if current.profile == profile && current.session_id.as_str() == session_id =>
            {
                false
            }
            _ => {
                self.by_node.insert(
                    node_id,
                    BoundAgentSession {
                        profile,
                        session_id: session_id.to_string(),
                    },
                );
                true
            }
        }
    }

    pub(crate) fn session_id(&self, node_id: NodeId) -> Option<&str> {
        self.by_node
            .get(&node_id)
            .map(|binding| binding.session_id.as_str())
    }

    pub(crate) fn clear(&mut self) {
        self.by_node.clear();
    }

    pub(crate) fn remove(&mut self, node_id: NodeId) {
        self.by_node.remove(&node_id);
    }
}

impl Runtime {
    pub fn hydrate_agent_session_bindings(
        &mut self,
        sessions: &[NodeSession],
    ) -> AgentSessionHydrationReport {
        self.agent_launch_cwds = sessions
            .iter()
            .filter_map(|session| {
                (session.kind == NodeSessionKind::Agent
                    && self.document.node(session.node_id).is_some())
                .then(|| {
                    session
                        .cwd
                        .as_ref()
                        .map(|cwd| (session.node_id, cwd.into()))
                })
                .flatten()
            })
            .collect();
        self.agent_session_bindings
            .hydrate(&self.document, sessions)
    }

    pub fn bind_discovered_agent_session(&mut self, node_id: NodeId, session_id: &str) -> bool {
        let Some(CanvasNodeKind::Agent(spec)) = self.document.node(node_id).map(|node| &node.kind)
        else {
            return false;
        };
        // A controlled card obtains its identity from explicit session creation
        // or its own saved binding, never a workspace-wide desktop discovery.
        if spec.mobile_control {
            return false;
        }
        self.agent_session_bindings
            .bind_if_unclaimed(node_id, spec.profile, session_id)
    }
}
