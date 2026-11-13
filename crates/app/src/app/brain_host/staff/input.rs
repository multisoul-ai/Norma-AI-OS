use super::*;
use crate::app::agent_runtime::command_support::{submit_agent_enter, type_agent_task};
use crate::app::staff::{
    delivery_action, delivery_confirmed, output_is_quiet, DeliveryAction, DeliveryStage,
};
use brain::DeliveryPhase;

impl MultisoulApp {
    pub(crate) fn confirm_staff_delivery(&mut self, node: u64) {
        let Some(pending) = self.staff.pending.get(&node) else {
            return;
        };
        let matches = self.runtime.staff_dispatch_id(node) == Some(pending.dispatch.as_str());
        if !delivery_confirmed(pending.stage, matches) {
            return;
        }
        let dispatch = pending.dispatch.clone();
        self.staff.pending.remove(&node);
        self.push_brain(BrainEventKind::WorkerDelivery {
            node_id: node,
            dispatch_id: dispatch,
            phase: DeliveryPhase::Accepted,
        });
    }

    pub(crate) fn tick_staff_workers(&mut self, now: Instant, cx: &mut Context<Self>) {
        let nodes: Vec<u64> = self.staff.pending.keys().copied().collect();
        for node in nodes {
            self.tick_staff_worker(node, now, cx);
        }
    }

    fn tick_staff_worker(&mut self, node: u64, now: Instant, cx: &mut Context<Self>) {
        let Some(pending) = self.staff.pending.get(&node) else {
            return;
        };
        let dispatch = pending.dispatch.clone();
        let prompt = pending.prompt.clone();
        let stage = pending.stage;
        if self.runtime.staff_dispatch_id(node) != Some(dispatch.as_str()) {
            self.staff.pending.remove(&node);
            return;
        }
        if self.runtime.terminal_has_exited(node) {
            self.staff.pending.remove(&node);
            self.staff_worker_failed(node, "staff.worker_failed".to_owned());
            self.notify_staff(cx);
            return;
        }
        let ready = self.runtime.agent_task_state(node) == Some(runtime::AgentTaskState::Ready);
        let quiet = output_is_quiet(self.runtime.last_output_at(node), now);
        let accepts_input = !self.runtime.agent_task_input_held(node);
        match delivery_action(stage, now, ready, quiet, true, accepts_input) {
            DeliveryAction::Wait => {}
            DeliveryAction::Type => self.type_staff_prompt(node, &prompt, now, cx),
            DeliveryAction::Enter => self.enter_staff_prompt(node, now, cx),
            DeliveryAction::NeedsYou => {
                self.staff.pending.remove(&node);
                self.push_brain(BrainEventKind::WorkerDelivery {
                    node_id: node,
                    dispatch_id: dispatch,
                    phase: DeliveryPhase::NeedsAttention,
                });
                self.notify_staff(cx);
            }
        }
    }

    fn type_staff_prompt(&mut self, node: u64, prompt: &str, now: Instant, cx: &mut Context<Self>) {
        let context = match self.shared_context_service() {
            Ok(context) => context,
            Err(error) => {
                self.fail_staff_input(node, error.to_string(), cx);
                return;
            }
        };
        let typed = context.execute_with_state(
            agent::context::ContextOperation::DispatchTask,
            None,
            |_| {
                type_agent_task(&mut self.runtime, node, prompt)
                    .map_err(|error| agent::AdapterError::Failed(error.to_string()))
            },
        );
        if let Err(error) = typed {
            self.fail_staff_input(node, error.to_string(), cx);
            return;
        }
        if let Some(pending) = self.staff.pending.get_mut(&node) {
            pending.stage = DeliveryStage::Typed { at: now };
        }
    }

    fn enter_staff_prompt(&mut self, node: u64, now: Instant, cx: &mut Context<Self>) {
        let context = match self.shared_context_service() {
            Ok(context) => context,
            Err(error) => {
                self.fail_staff_input(node, error.to_string(), cx);
                return;
            }
        };
        let entered = context.execute_with_state(
            agent::context::ContextOperation::DispatchTask,
            None,
            |_| {
                submit_agent_enter(&mut self.runtime, node)
                    .map_err(|error| agent::AdapterError::Failed(error.to_string()))
            },
        );
        if let Err(error) = entered {
            self.fail_staff_input(node, error.to_string(), cx);
            return;
        }
        if let Some(pending) = self.staff.pending.get_mut(&node) {
            pending.stage = DeliveryStage::Submitted { at: now };
        }
    }

    pub(crate) fn redeliver_staff_worker(
        &mut self,
        node: u64,
        dispatch: &str,
        cx: &mut Context<Self>,
    ) {
        if self.staff.busy || self.staff.pending.contains_key(&node) {
            return;
        }
        let prompt = self.staff.tasks.values().find_map(|task| {
            let worker = task.workers.iter().find(|worker| {
                worker.card_id == Some(node)
                    && worker.dispatch_id == dispatch
                    && worker.state == norma::protocol::WorkerState::NeedsAttention
            })?;
            let prompt = if worker.launch_prompt.is_empty() {
                task.proposals
                    .iter()
                    .find(|proposal| proposal.dispatch_id.as_deref() == Some(dispatch))
                    .map(|proposal| proposal.prompt.clone())
                    .unwrap_or_default()
            } else {
                worker.launch_prompt.clone()
            };
            Some(prompt)
        });
        let Some(prompt) = prompt.filter(|prompt| !prompt.is_empty()) else {
            return;
        };
        if self.runtime.staff_dispatch_id(node) != Some(dispatch) {
            return;
        }
        if self.runtime.terminal_has_exited(node) {
            self.staff_worker_failed(node, "staff.worker_failed".to_owned());
            self.notify_staff(cx);
            return;
        }
        self.staff.pending.insert(
            node,
            crate::app::staff::PendingWorker {
                dispatch: dispatch.to_owned(),
                prompt,
                stage: DeliveryStage::Starting {
                    since: Instant::now(),
                    grace_spent: true,
                },
            },
        );
        self.push_brain(BrainEventKind::WorkerDelivery {
            node_id: node,
            dispatch_id: dispatch.to_owned(),
            phase: DeliveryPhase::Redeliver,
        });
        self.tick_staff_worker(node, Instant::now(), cx);
        self.notify_staff(cx);
    }

    fn fail_staff_input(&mut self, node: u64, reason: String, cx: &mut Context<Self>) {
        self.staff.pending.remove(&node);
        self.staff_worker_failed(node, reason);
        self.notify_staff(cx);
    }

    pub(super) fn staff_worker_failed(&self, node: u64, reason: String) {
        tracing::warn!(node, %reason, "staff worker could not start or accept input");
        self.push_brain(BrainEventKind::AgentFinished {
            node_id: node,
            dispatch_id: self.runtime.staff_dispatch_id(node).map(str::to_owned),
            agent: self.agent_announcement_name(node).unwrap_or_default(),
            outcome: Outcome::Failed,
            excerpt: "staff.worker_failed".into(),
            from_result: true,
        });
    }
}
