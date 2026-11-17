//! Authenticated task commands run disk IO off the GPUI thread.
use super::*;
use norma::protocol::StaffOp;
use std::sync::Arc;

mod assign;
mod decisions;
mod dispatch;
mod events;
mod input;
mod pick;
mod resume;
mod worker_questions;

impl MultisoulApp {
    pub(crate) fn staff_store(&self) -> Result<Arc<brain::staff::TaskStore>, String> {
        self.staff_ready().map_err(str::to_owned)?;
        self.brain
            .client
            .as_ref()
            .and_then(|client| client.tasks())
            .ok_or_else(|| "island.staff_loading".to_owned())
    }

    pub(crate) fn start_staff_command(
        &mut self,
        token: &str,
        op: StaffOp,
        cx: &mut Context<Self>,
    ) -> Result<gpui::Task<Result<String, String>>, String> {
        if !self.brain.token_matches(token) {
            return Err("staff.invalid_source".to_owned());
        }
        if let StaffOp::Adopt {
            task_id,
            proposal_id,
        } = op
        {
            return self.dispatch_staff_worker(task_id, proposal_id, token.to_owned(), cx);
        }
        if let StaffOp::Propose { agent, .. } | StaffOp::Assign { agent, .. } = &op {
            self.resolve_staff_entry(agent)?;
        }
        if matches!(op, StaffOp::Assign { .. }) {
            return self.assign_staff_worker(op, token.to_owned(), cx);
        }
        self.write_staff_operation(op, false, cx)
    }

    /// 幕僚用 engine（如 claude）或旧的具名智能体名指向执行者。
    /// 可派类型只看编排里注册成功的引擎；同类型优先挑不忙的名册项。
    /// 没有名册时仍可派，宿主会按引擎另开一张卡片。找不到时列出可派引擎。
    pub(super) fn resolve_staff_entry(&self, id: &str) -> Result<agent::AgentRosterEntry, String> {
        let registered = self.registered_staff_engines();
        if let Some(provider) = registered
            .iter()
            .copied()
            .find(|provider| provider.slug().eq_ignore_ascii_case(id))
        {
            return Ok(self.staff_engine_binding(provider));
        }
        if let Some(entry) = self.agent_roster_for_provider().into_iter().find(|entry| {
            entry.enabled
                && entry.name.eq_ignore_ascii_case(id)
                && registered
                    .iter()
                    .any(|provider| provider.profile() == entry.profile)
        }) {
            return Ok(entry);
        }
        let engines = registered
            .iter()
            .map(|provider| provider.slug())
            .collect::<Vec<_>>()
            .join(", ");
        Err(format!("staff.agent_unavailable\n{engines}"))
    }

    fn staff_engine_binding(
        &self,
        provider: norma::orchestration::OrchestrationProvider,
    ) -> agent::AgentRosterEntry {
        let profile = provider.profile();
        let mut found: Vec<_> = self
            .agent_roster_for_provider()
            .into_iter()
            .filter(|entry| entry.enabled && entry.profile == profile)
            .collect();
        // 幕僚派出的卡不挂 definition_id，busy 看不到它们；按同名卡数量分散，避免全派给同一个名字。
        let load = |entry: &agent::AgentRosterEntry| {
            self.runtime
                .document
                .nodes()
                .iter()
                .filter(|node| {
                    matches!(&node.kind, canvas::CanvasNodeKind::Agent(spec)
                        if spec.alias.as_deref() == Some(entry.name.as_str()))
                })
                .count()
        };
        let mut candidates: Vec<_> = found.iter().collect();
        pick::least_loaded(&mut candidates, load)
            .cloned()
            .unwrap_or_else(|| agent::AgentRosterEntry {
                definition_id: 0,
                name: provider.display_name().to_owned(),
                speech_alias: None,
                profile,
                enabled: true,
                active_node_id: None,
                tracking: agent::AgentTrackingMode::Untracked,
                busy: false,
                task_state: agent::AgentTaskState::Ready,
            })
    }

    pub(crate) fn write_staff_operation(
        &mut self,
        op: StaffOp,
        manual: bool,
        cx: &mut Context<Self>,
    ) -> Result<gpui::Task<Result<String, String>>, String> {
        let store = self.staff_store()?;
        let scope = self
            .brain_scope()
            .ok_or_else(|| "island.staff_no_workspace".to_owned())?;
        let wake = matches!(op, StaffOp::Message { .. })
            .then(|| op.task_id().unwrap_or_default().to_owned());
        let workspace = scope.workspace_id.clone();
        let work = cx.background_executor().spawn(async move {
            if manual {
                if let StaffOp::Phase { task_id, phase } = op {
                    return store
                        .update(&workspace, &task_id, |task| {
                            task.set_phase(phase, true);
                            Ok(())
                        })
                        .map(|task| vec![task]);
                }
            }
            store.execute(&workspace, op)
        });
        Ok(cx.spawn(async move |app, cx| {
            let tasks = work.await.map_err(|error| error.to_string())?;
            let output = serde_json::to_string(&tasks).map_err(|error| error.to_string())?;
            app.update(cx, |app, cx| {
                if app.brain_scope().as_ref() != Some(&scope) {
                    return;
                }
                app.receive_staff_tasks(tasks, cx);
                if let Some(task_id) = wake {
                    app.push_brain(BrainEventKind::TaskRequest { task_id });
                }
            })
            .map_err(|_| "staff.invalid_source".to_owned())?;
            Ok(output)
        }))
    }

    pub(crate) fn load_staff_tasks(&mut self, cx: &mut Context<Self>) {
        if self.staff.loaded || self.staff.loading || self.staff.error.is_some() {
            return;
        }
        match self.write_staff_operation(StaffOp::List, false, cx) {
            Ok(work) => {
                self.staff.loading = true;
                cx.spawn(async move |app, cx| {
                    let result = work.await;
                    let _ = app.update(cx, |app, cx| {
                        app.staff.loading = false;
                        app.staff.error = result.err().map(crate::app::staff::error_message);
                        app.notify_staff(cx);
                    });
                })
                .detach();
            }
            // The shared service starts asynchronously. This is not a failed list request.
            Err(error) if error == "island.staff_loading" => {}
            Err(error) => self.staff.error = Some(crate::app::staff::error_message(error)),
        }
    }
}
