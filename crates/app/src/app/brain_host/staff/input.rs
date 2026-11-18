use super::*;
use crate::app::agent_runtime::command_support::{submit_agent_enter, type_agent_task};
use crate::app::staff::{output_is_quiet, quiet_allows_typing, startup_can_type, DeliveryClock};
use brain::staff::worker_life::{step, Effect, Input};
use brain::DeliveryPhase;
use norma::protocol::{WorkerFailure, WorkerPhase};

impl MultisoulApp {
    pub(crate) fn confirm_staff_delivery(&mut self, node: u64) {
        if let Some(clock) = self.staff.clocks.get(&node) {
            if self.runtime.staff_dispatch_id(node) != Some(clock.dispatch.as_str()) {
                return;
            }
            if !matches!(clock.phase, WorkerPhase::Submitted { .. }) {
                return;
            }
            let dispatch = clock.dispatch.clone();
            self.staff.clocks.remove(&node);
            self.note_staff_phase(node, &dispatch, WorkerPhase::Running);
            self.push_brain(BrainEventKind::WorkerDelivery {
                node_id: node,
                dispatch_id: dispatch,
                phase: DeliveryPhase::Accepted,
            });
            return;
        }
        self.recover_late_delivery(node);
    }

    fn staff_agent_profile(&self, node: u64) -> Option<canvas::AgentProfile> {
        match self.runtime.document.node(node).map(|node| &node.kind) {
            Some(canvas::CanvasNodeKind::Agent(spec)) => Some(spec.profile),
            _ => None,
        }
    }

    /// 15 秒窗口过后才到的 UserPromptSubmit：卡片其实收到了任务，把「未送达」收回。
    fn recover_late_delivery(&mut self, node: u64) {
        let dispatch = self.staff.tasks.values().find_map(|task| {
            task.workers.iter().find_map(|worker| {
                (worker.card_id == Some(node)
                    && matches!(
                        worker.phase,
                        WorkerPhase::Failed(WorkerFailure::Unconfirmed)
                    )
                    && self.runtime.staff_dispatch_id(node) == Some(worker.dispatch_id.as_str()))
                .then(|| worker.dispatch_id.clone())
            })
        });
        let Some(dispatch) = dispatch else {
            return;
        };
        tracing::info!(node, %dispatch, "staff delivery confirmed after timeout; clearing needs-attention");
        self.note_staff_phase(node, &dispatch, WorkerPhase::Running);
        self.push_brain(BrainEventKind::WorkerDelivery {
            node_id: node,
            dispatch_id: dispatch,
            phase: DeliveryPhase::Accepted,
        });
    }

    pub(crate) fn tick_staff_workers(&mut self, now: Instant, cx: &mut Context<Self>) {
        let nodes: Vec<u64> = self.staff.clocks.keys().copied().collect();
        for node in nodes {
            self.tick_staff_worker(node, now, cx);
        }
    }

    fn tick_staff_worker(&mut self, node: u64, now: Instant, cx: &mut Context<Self>) {
        let Some(clock) = self.staff.clocks.get(&node).cloned() else {
            return;
        };
        if self.runtime.staff_dispatch_id(node) != Some(clock.dispatch.as_str()) {
            self.staff.clocks.remove(&node);
            return;
        }
        if self.runtime.terminal_has_exited(node) {
            self.staff.clocks.remove(&node);
            self.staff_worker_failed(node, "staff.worker_failed".to_owned());
            self.notify_staff(cx);
            return;
        }
        let elapsed = now.saturating_duration_since(clock.since);
        let ready = self.runtime.agent_task_state(node) == Some(runtime::AgentTaskState::Ready);
        let quiet = quiet_allows_typing(
            self.staff_agent_profile(node),
            self.runtime.agent_task_state(node).is_some(),
            output_is_quiet(self.runtime.last_output_at(node), now),
        );
        let accepts_input = !self.runtime.agent_task_input_held(node);
        let input = if clock.phase == WorkerPhase::Spawning
            && startup_can_type(
                elapsed,
                clock.grace_spent,
                ready,
                quiet,
                accepts_input,
                true,
            ) {
            Input::OutputQuiet
        } else {
            Input::Tick {
                elapsed,
                accepts_input,
            }
        };
        let (next, effects) = step(clock.phase, input);
        if next == clock.phase && effects.is_empty() {
            return;
        }
        if matches!(next, WorkerPhase::Failed(WorkerFailure::Unconfirmed)) {
            tracing::warn!(node, dispatch = %clock.dispatch, "staff prompt not confirmed within the window");
            self.staff.clocks.remove(&node);
            self.note_staff_phase(node, &clock.dispatch, next);
            self.push_brain(BrainEventKind::WorkerDelivery {
                node_id: node,
                dispatch_id: clock.dispatch,
                phase: DeliveryPhase::NeedsAttention,
            });
            self.notify_staff(cx);
            return;
        }
        for effect in effects {
            let ok = match effect {
                Effect::Type => self.type_staff_prompt(node, &clock.prompt, cx),
                Effect::PressEnter => {
                    if matches!(next, WorkerPhase::Submitted { retried: true }) {
                        tracing::info!(node, "staff prompt unconfirmed; retrying Enter once");
                    }
                    self.enter_staff_prompt(node, cx)
                }
                Effect::CreateCard
                | Effect::Spawn
                | Effect::Interrupt
                | Effect::CloseCard
                | Effect::WakeStaff => true,
            };
            if !ok {
                return;
            }
        }
        let keep_since = matches!(
            (clock.phase, next),
            (
                WorkerPhase::Submitted { retried: false },
                WorkerPhase::Submitted { retried: true }
            )
        );
        if let Some(stored) = self.staff.clocks.get_mut(&node) {
            stored.phase = next;
            if !keep_since {
                stored.since = now;
            }
        }
        self.note_staff_phase(node, &clock.dispatch, next);
    }

    fn type_staff_prompt(&mut self, node: u64, prompt: &str, cx: &mut Context<Self>) -> bool {
        let context = match self.shared_context_service() {
            Ok(context) => context,
            Err(error) => {
                self.fail_staff_input(node, error.to_string(), cx);
                return false;
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
            return false;
        }
        true
    }

    fn enter_staff_prompt(&mut self, node: u64, cx: &mut Context<Self>) -> bool {
        let context = match self.shared_context_service() {
            Ok(context) => context,
            Err(error) => {
                self.fail_staff_input(node, error.to_string(), cx);
                return false;
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
            return false;
        }
        true
    }

    pub(crate) fn redeliver_staff_worker(
        &mut self,
        node: u64,
        dispatch: &str,
        cx: &mut Context<Self>,
    ) {
        if self.staff.busy || self.staff.clocks.contains_key(&node) {
            return;
        }
        let prompt = self.staff.tasks.values().find_map(|task| {
            let worker = task.workers.iter().find(|worker| {
                worker.card_id == Some(node)
                    && worker.dispatch_id == dispatch
                    && matches!(
                        worker.phase,
                        norma::protocol::WorkerPhase::Failed(
                            norma::protocol::WorkerFailure::Unconfirmed
                        )
                    )
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
        self.arm_staff_delivery(node, dispatch.to_owned(), prompt, true);
        self.push_brain(BrainEventKind::WorkerDelivery {
            node_id: node,
            dispatch_id: dispatch.to_owned(),
            phase: DeliveryPhase::Redeliver,
        });
        self.tick_staff_worker(node, Instant::now(), cx);
        self.notify_staff(cx);
    }

    fn fail_staff_input(&mut self, node: u64, reason: String, cx: &mut Context<Self>) {
        self.staff.clocks.remove(&node);
        self.staff_worker_failed(node, reason);
        self.notify_staff(cx);
    }

    pub(crate) fn arm_staff_delivery(
        &mut self,
        node: u64,
        dispatch: String,
        prompt: String,
        grace_spent: bool,
    ) {
        let current = self
            .staff
            .tasks
            .values()
            .find_map(|task| {
                task.workers
                    .iter()
                    .find(|worker| worker.card_id == Some(node) && worker.dispatch_id == dispatch)
            })
            .map(|worker| worker.phase)
            .unwrap_or(WorkerPhase::Reserved);
        let (phase, _) = step(current, Input::Spawned);
        self.staff.clocks.insert(
            node,
            DeliveryClock {
                dispatch: dispatch.clone(),
                prompt,
                since: Instant::now(),
                grace_spent,
                phase,
            },
        );
        self.note_staff_phase(node, &dispatch, phase);
    }

    fn note_staff_phase(&mut self, node: u64, dispatch: &str, phase: WorkerPhase) {
        for task in self.staff.tasks.values_mut() {
            if let Some(worker) = task.workers.iter_mut().find(|worker| {
                worker.card_id == Some(node)
                    && worker.dispatch_id == dispatch
                    && worker.outcome().is_none()
            }) {
                worker.phase = phase;
            }
        }
    }

    pub(crate) fn overlay_delivery_clocks(&mut self) {
        let clocks: Vec<_> = self
            .staff
            .clocks
            .iter()
            .map(|(node, clock)| (*node, clock.dispatch.clone(), clock.phase))
            .collect();
        for (node, dispatch, phase) in clocks {
            self.note_staff_phase(node, &dispatch, phase);
        }
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
