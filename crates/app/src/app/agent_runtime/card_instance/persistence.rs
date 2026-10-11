//! Memory-only journal bridge. The shared service commits Armed before installing an input.
use super::*;
use crate::workspace::dispatch::journal::WorkspaceJournal;
use std::collections::VecDeque;
use storage::{StoredWorkspaceAssignment, WorkspaceInputPhase};

#[derive(Default)]
pub(super) struct DurableCardInstanceEvents {
    journals: HashMap<String, WorkspaceJournal>,
    pending: VecDeque<CardInstanceEvent>,
    pub(super) legacy_events: VecDeque<CardInstanceEvent>,
}

impl MultisoulApp {
    /// Trusted workspace service adapter: the row comes from its successful Armed commit.
    /// No database access or PTY write occurs here; await boundaries cannot refresh the target.
    pub(crate) fn accept_persisted_card_instance_assignment(
        &mut self,
        item: &StoredWorkspaceAssignment,
        authority: Arc<AtomicBool>,
        journal: WorkspaceJournal,
    ) -> Result<CardInstanceReceipt, CardInstanceDeliveryError> {
        let card = &item.assignment.target.card;
        if item.input_phase != WorkspaceInputPhase::Armed
            || item.receipt.stage != CardInstanceStage::Accepted
        {
            return Err(delivery_error("input_not_freshly_armed", true));
        }
        if self.workspace_role_scope() != Some((&card.workspace_id, &card.area_id)) {
            return Err(delivery_error("workspace_scope_changed", true));
        }
        let id = item.assignment.assignment_id.clone();
        if self.card_instance_tasks.durable.journals.contains_key(&id) {
            return self
                .card_instance_receipt(&id)
                .ok_or_else(|| delivery_error("assignment_missing", true));
        }
        // watch closes the gate if the background writer has failed, before queue installation.
        journal.watch(&id, authority.clone());
        let receipt = self.accept_guarded_card_instance_assignment(
            &card.workspace_id,
            &card.area_id,
            item.assignment.target.execution.route_generation,
            item.assignment.clone(),
            authority,
        )?;
        self.card_instance_tasks
            .durable
            .journals
            .insert(id, journal);
        Ok(receipt)
    }

    /// Called after the frame's input/Hook updates. Enqueue only; never fall back to foreground IO.
    pub(crate) fn journal_workspace_card_instance_events(&mut self) {
        if self.card_instance_tasks.durable.journals.is_empty() {
            return;
        }
        // Keep legacy consumers' events intact. Only durable assignments belong to this writer.
        for event in self.card_instance_tasks.coordinator.take_events() {
            self.card_instance_tasks
                .durable
                .legacy_events
                .push_back(event.clone());
            if self
                .card_instance_tasks
                .durable
                .journals
                .contains_key(&event.receipt.assignment_id)
            {
                self.card_instance_tasks.durable.pending.push_back(event);
            }
        }
        while let Some(event) = self.card_instance_tasks.durable.pending.pop_front() {
            let id = event.receipt.assignment_id.clone();
            let Some(journal) = self.card_instance_tasks.durable.journals.get(&id) else {
                self.card_instance_tasks.durable.pending.push_front(event);
                break;
            };
            journal.enqueue(event);
        }
    }
}
