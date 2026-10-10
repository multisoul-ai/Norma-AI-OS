//! Exclusive, instance-bound prompt transport. No storage, process startup or queue restoration.
use super::*;
use norma::workspace_dispatch::WorkspaceCardTarget;
use std::collections::HashSet;
use terminal::{Key, KeyInput, TerminalStatus};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkspaceCardInputError {
    pub code: &'static str,
    pub zero_input_proven: bool,
}
impl std::fmt::Display for WorkspaceCardInputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code)
    }
}
impl std::error::Error for WorkspaceCardInputError {}

#[derive(Clone, Copy, PartialEq)]
enum InputStage {
    Reserved,
    Typed,
    Unknown,
}
struct Reservation {
    owner: String,
    target: WorkspaceCardTarget,
    text: String,
    stage: InputStage,
}
#[derive(Default)]
pub(super) struct WorkspaceInputs {
    reservations: HashMap<NodeId, Reservation>,
    drafts: HashSet<NodeId>,
}
impl WorkspaceInputs {
    pub(super) fn remove(&mut self, node: NodeId) {
        self.reservations.remove(&node);
        self.drafts.remove(&node);
    }
    pub(super) fn clear(&mut self) {
        self.reservations.clear();
        self.drafts.clear();
    }
    pub(super) fn blocks_human(&self, node: NodeId) -> bool {
        self.reservations
            .get(&node)
            .is_some_and(|r| r.stage != InputStage::Unknown)
    }
    pub(super) fn observe_user_input(&mut self, node: NodeId, input: &NodeInput, successful: bool) {
        let enter = matches!(input, NodeInput::Key(key) if key.key == Key::Enter && !key.ctrl && !key.alt && !key.shift);
        if enter && successful {
            self.drafts.remove(&node);
        } else if !matches!(input, NodeInput::Text(s) | NodeInput::Paste(s) if s.is_empty())
            && !matches!(input, NodeInput::Key(key) if matches!(key.key, Key::Escape | Key::Backspace | Key::Left | Key::Right) || key.ctrl && key.key == Key::Char('c'))
        {
            self.drafts.insert(node);
        }
    }
}

impl Runtime {
    /// Includes unknown drafts: automation stays blocked while humans may repair an uncertain composer.
    pub fn workspace_card_input_held(&self, node: NodeId) -> bool {
        self.workspace_inputs.reservations.contains_key(&node)
            || self.workspace_card_provider_input_pending(node)
    }

    pub fn workspace_card_user_input_pending(&self, node: NodeId) -> bool {
        self.workspace_inputs.drafts.contains(&node)
            || self.workspace_card_provider_input_inflight(node)
    }

    /// App must first authorize the area/Runtime binding and capture its ready execution fingerprint.
    pub fn reserve_workspace_card_input(
        &mut self,
        target: &WorkspaceCardTarget,
        owner: &str,
        text: &str,
    ) -> Result<(), WorkspaceCardInputError> {
        self.check_workspace_input_target(target, true)?;
        if owner.is_empty()
            || text.trim().is_empty()
            || text.len() > 32 * 1024
            || text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err(input_error("invalid_message", true));
        }
        let node = target.card.node_id;
        if self.workspace_card_uses_rpc(node) {
            return Err(input_error("provider_rpc_required", true));
        }
        if self.workspace_card_input_held(node) || self.mobile_terminal_is_controlled(node) {
            return Err(input_error("input_owned", true));
        }
        if self.workspace_inputs.drafts.contains(&node)
            || self.workspace_card_provider_input_inflight(node)
        {
            return Err(input_error("desktop_input_pending", true));
        }
        if matches!(
            self.agent_task_state(node),
            Some(crate::AgentTaskState::WaitingApproval { .. })
        ) {
            return Err(input_error("waiting_approval", true));
        }
        if text.contains(['\n', '\t'])
            && !self
                .live
                .get(&node)
                .is_some_and(|live| live.terminal_bracketed_paste_enabled())
        {
            return Err(input_error("terminal_not_ready", true));
        }
        self.workspace_inputs.reservations.insert(
            node,
            Reservation {
                owner: owner.into(),
                target: target.clone(),
                text: text.into(),
                stage: InputStage::Reserved,
            },
        );
        Ok(())
    }

    pub fn paste_workspace_card_input(
        &mut self,
        target: &WorkspaceCardTarget,
        owner: &str,
        text: &str,
    ) -> Result<(), WorkspaceCardInputError> {
        self.check_workspace_reservation(target, owner, InputStage::Reserved)?;
        self.check_workspace_input_target(target, true)?;
        let node = target.card.node_id;
        if self.workspace_inputs.reservations[&node].text != text {
            return Err(input_error("input_conflict", true));
        }
        if matches!(
            self.agent_task_state(node),
            Some(crate::AgentTaskState::WaitingApproval { .. })
        ) {
            return Err(input_error("waiting_approval", true));
        }
        // Mark before write: an IO error can follow a partial PTY write.
        self.workspace_inputs
            .reservations
            .get_mut(&node)
            .expect("reserved")
            .stage = InputStage::Typed;
        if self
            .write_node_input(node, NodeInput::Paste(text.into()))
            .is_err()
        {
            self.mark_workspace_card_input_unknown(target, owner);
            return Err(input_error("outcome_unknown", false));
        }
        Ok(())
    }

    pub fn enter_workspace_card_input(
        &mut self,
        target: &WorkspaceCardTarget,
        owner: &str,
    ) -> Result<(), WorkspaceCardInputError> {
        self.check_workspace_reservation(target, owner, InputStage::Typed)?;
        if let Err(error) = self.check_workspace_input_target(target, false) {
            self.mark_workspace_card_input_unknown(target, owner);
            return Err(error);
        }
        let node = target.card.node_id;
        if matches!(
            self.agent_task_state(node),
            Some(crate::AgentTaskState::WaitingApproval { .. })
        ) {
            self.mark_workspace_card_input_unknown(target, owner);
            return Err(input_error("approval_after_paste", false));
        }
        let input = NodeInput::Key(KeyInput {
            key: Key::Enter,
            ctrl: false,
            alt: false,
            shift: false,
        });
        if self.write_node_input(node, input).is_err() {
            self.mark_workspace_card_input_unknown(target, owner);
            return Err(input_error("outcome_unknown", false));
        }
        self.workspace_inputs.reservations.remove(&node);
        Ok(())
    }

    /// Cancel only before any write. Never clear a draft on a replaced card/session.
    pub fn release_unwritten_workspace_card_input(
        &mut self,
        target: &WorkspaceCardTarget,
        owner: &str,
    ) -> Result<(), WorkspaceCardInputError> {
        self.check_workspace_reservation(target, owner, InputStage::Reserved)?;
        self.workspace_inputs
            .reservations
            .remove(&target.card.node_id);
        Ok(())
    }

    pub fn mark_workspace_card_input_unknown(&mut self, target: &WorkspaceCardTarget, owner: &str) {
        let node = target.card.node_id;
        if let Some(slot) = self.workspace_inputs.reservations.get_mut(&node) {
            if slot.owner == owner && slot.target == *target {
                slot.stage = InputStage::Unknown;
            }
            return;
        }
        // A lost RPC receipt may have no paste lease. Quarantine only the unchanged original instance.
        if self
            .validate_workspace_card_target(
                &target.card.workspace_id,
                &target.card.area_id,
                target.execution.route_generation,
                target,
            )
            .is_ok()
        {
            self.workspace_inputs.reservations.insert(
                node,
                Reservation {
                    owner: owner.into(),
                    target: target.clone(),
                    text: String::new(),
                    stage: InputStage::Unknown,
                },
            );
        }
    }

    fn check_workspace_input_target(
        &self,
        target: &WorkspaceCardTarget,
        zero: bool,
    ) -> Result<(), WorkspaceCardInputError> {
        self.validate_workspace_card_target(
            &target.card.workspace_id,
            &target.card.area_id,
            target.execution.route_generation,
            target,
        )
        .map_err(|e| input_error(e.code(), zero))?;
        if !self
            .live
            .get(&target.card.node_id)
            .is_some_and(|live| live.terminal_status() == Some(TerminalStatus::Running))
        {
            return Err(input_error("terminal_unavailable", zero));
        }
        Ok(())
    }
    fn check_workspace_reservation(
        &self,
        target: &WorkspaceCardTarget,
        owner: &str,
        stage: InputStage,
    ) -> Result<(), WorkspaceCardInputError> {
        let Some(slot) = self.workspace_inputs.reservations.get(&target.card.node_id) else {
            return Err(input_error("input_not_owned", true));
        };
        if slot.owner != owner || slot.target != *target || slot.stage != stage {
            return Err(input_error(
                "input_conflict",
                slot.stage == InputStage::Reserved,
            ));
        }
        Ok(())
    }
}
fn input_error(code: &'static str, zero_input_proven: bool) -> WorkspaceCardInputError {
    WorkspaceCardInputError {
        code,
        zero_input_proven,
    }
}
