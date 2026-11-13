use super::*;
use crate::app::agent_runtime::command_support::{submit_agent_enter, type_agent_task};

impl MultisoulApp {
    pub(crate) fn submit_staff_ready_worker(&mut self, node: u64, cx: &mut Context<Self>) {
        let Some(pending) = self.staff.pending.remove(&node) else {
            return;
        };
        if self.runtime.staff_dispatch_id(node) != Some(&pending.dispatch) {
            return;
        }
        let context = match self.shared_context_service() {
            Ok(context) => context,
            Err(error) => {
                self.staff_worker_failed(node, error.to_string());
                return;
            }
        };
        let typed = context.execute_with_state(
            agent::context::ContextOperation::DispatchTask,
            None,
            |_| {
                type_agent_task(&mut self.runtime, node, &pending.prompt)
                    .map_err(|error| agent::AdapterError::Failed(error.to_string()))
            },
        );
        if let Err(error) = typed {
            self.staff_worker_failed(node, error.to_string());
            return;
        }
        cx.spawn(async move |app, cx| {
            // Match the existing terminal dispatch delay: let paste settle before sending Enter.
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
            let _ = app.update(cx, |app, cx| {
                if app.runtime.staff_dispatch_id(node) != Some(&pending.dispatch) {
                    return;
                }
                let result = context.execute_with_state(
                    agent::context::ContextOperation::DispatchTask,
                    None,
                    |_| {
                        submit_agent_enter(&mut app.runtime, node)
                            .map_err(|error| agent::AdapterError::Failed(error.to_string()))
                    },
                );
                if let Err(error) = result {
                    app.staff_worker_failed(node, error.to_string());
                }
                app.notify_staff(cx);
            });
        })
        .detach();
    }

    pub(crate) fn expire_staff_workers(&mut self, cx: &mut Context<Self>) {
        // A missing ready hook is not permission to type into a startup/consent screen.
        // 60 seconds = twice the existing 30-second startup grace, then require human inspection.
        let expired: Vec<_> = self
            .staff
            .pending
            .iter()
            .filter(|(_, p)| p.since.elapsed() >= Duration::from_secs(60))
            .map(|(node, _)| *node)
            .collect();
        for node in expired {
            self.staff.pending.remove(&node);
            self.staff_worker_failed(node, "staff.worker_not_ready".to_owned());
            self.notify_staff(cx);
        }
    }

    pub(super) fn staff_worker_failed(&self, node: u64, reason: String) {
        tracing::warn!(node, %reason, "staff worker could not start or accept input");
        self.push_brain(BrainEventKind::AgentFinished {
            node_id: node,
            dispatch_id: self.runtime.staff_dispatch_id(node).map(str::to_owned),
            agent: self.agent_announcement_name(node).unwrap_or_default(),
            outcome: Outcome::Failed,
            excerpt: if reason == "staff.worker_not_ready" {
                reason
            } else {
                "staff.worker_failed".into()
            },
            from_result: true,
        });
    }
}
