use super::*;
use crate::app::staff::PendingWorker;
use agent::context::ContextOperation;
use norma::protocol::{StaffProposal, Worker};

impl MultisoulApp {
    pub(super) fn dispatch_staff_worker(
        &mut self,
        task_id: String,
        proposal_id: u64,
        token: String,
        cx: &mut Context<Self>,
    ) -> Result<gpui::Task<Result<String, String>>, String> {
        let store = self.staff_store()?;
        let scope = self
            .brain_scope()
            .ok_or_else(|| "staff.invalid_source".to_owned())?;
        let context = self
            .prepare_brain_context(&token)
            .map_err(|error| error.to_string())?;
        let reader = store.clone();
        let workspace = scope.workspace_id.clone();
        let id = task_id.clone();
        let read = cx
            .background_executor()
            .spawn(async move { reader.get(&workspace, &id) });
        Ok(cx.spawn(async move |entity, cx| {
            let task = read.await.map_err(|error| error.to_string())?;
            let proposal = task.proposals.iter().find(|p| p.id == proposal_id)
                .cloned().ok_or_else(|| "staff.proposal_unavailable".to_owned())?;
            if !proposal.approved { return Err("staff.approval_required".to_owned()); }
            // Idempotent CLI retries return the existing dispatch; never launch again.
            if let Some(dispatch) = &proposal.dispatch_id {
                return Ok(serde_json::json!({"task":task.id,"dispatch_id":dispatch,"status":"already_dispatched"}).to_string());
            }
            context.snapshot().await.map_err(|error| error.to_string())?;
            let current_scope = scope.clone();
            let current_token = token.clone();
            let reserved = entity.update(cx, |app, cx| {
                app.check_staff_source(&current_token, &current_scope)?;
                context.execute_with_state(ContextOperation::DispatchTask, None, |state| {
                    let prompt = agent::context::context_task_handoff(&proposal.prompt, &[("source_and_destination", state)])
                        .map_err(|reason| agent::AdapterError::Rejected(reason.into()))?;
                    app.reserve_staff_worker(&proposal, prompt, cx).map_err(agent::AdapterError::Rejected)
                }).map_err(|error| error.to_string())
            }).map_err(|_| "staff.invalid_source".to_owned())??;
            let (worker, prompt) = reserved;
            let node = worker.card_id.expect("host reserved card");
            let dispatch = worker.dispatch_id.clone();
            let worker_agent = worker.agent.clone();
            let writer = store.clone();
            let workspace = scope.workspace_id.clone();
            let id = task_id.clone();
            let bound = cx.background_executor().spawn(async move { writer.bind_worker(&workspace, &id, proposal_id, worker) }).await;
            let bound = match bound {
                Ok(task) => task,
                Err(error) => {
                    let _ = entity.update(cx, |app, cx| { app.runtime.remove_node(node); app.notify_staff(cx); });
                    return Err(error.to_string());
                }
            };
            let prepared = entity.update(cx, |app, cx| {
                app.check_staff_source(&token, &scope)?;
                app.receive_staff_tasks(vec![bound], cx);
                // Recheck effective restrictions immediately before accepting the launch.
                context.execute_with_state(ContextOperation::DispatchTask, None, |_| {
                    app.runtime.prepare_native_agent_terminal(node, 80, 24)
                        .map_err(|error| agent::AdapterError::Failed(error.to_string()))
                }).map_err(|error| error.to_string())
            }).map_err(|_| "staff.invalid_source".to_owned()).and_then(|result| result);
            let result = match prepared {
                Ok(launch) => {
                    let run = launch.run_id().to_owned();
                    let spawned = cx.background_executor().spawn(async move { launch.spawn() }).await;
                    entity.update(cx, |app, cx| {
                        app.check_staff_source(&token, &scope)?;
                        let live = spawned.and_then(|prepared| prepared.into_live());
                        app.runtime.complete_native_agent_terminal(node, &run, live).map_err(|error| error.to_string())?;
                        app.brain.own_prompts.remember(&prompt);
                        app.staff.pending.insert(node, PendingWorker { dispatch: dispatch.clone(), prompt, since: Instant::now() });
                        if app.runtime.agent_task_state(node) == Some(runtime::AgentTaskState::Ready) {
                            app.submit_staff_ready_worker(node, cx);
                        }
                        app.mark_canvas_dirty();
                        app.mark_session_dirty();
                        app.notify_staff(cx);
                        Ok(())
                    }).map_err(|_| "staff.invalid_source".to_owned()).and_then(|result| result)
                }
                Err(error) => Err(error),
            };
            if let Err(error) = result {
                tracing::warn!(node, %error, "staff launch failed after binding");
                let failure = BrainEvent { scope: scope.clone(), kind: BrainEventKind::AgentFinished {
                    node_id: node, dispatch_id: Some(dispatch.clone()), agent: worker_agent,
                    outcome: Outcome::Failed, excerpt: "staff.worker_failed".into(), from_result: true,
                }};
                if entity.update(cx, |app, cx| {
                    app.staff.pending.remove(&node);
                    app.staff.events.borrow_mut().push_back(failure.clone());
                    app.save_staff_events(store.clone(), cx);
                    app.notify_staff(cx);
                }).is_err() {
                    // The owning view can disappear during spawn. Captured identity still settles its binding.
                    loop {
                        let writer = store.clone();
                        let event = failure.clone();
                        if cx.background_executor().spawn(async move { writer.record_event(&event.scope.workspace_id, &event.kind) }).await.is_ok() { break; }
                        cx.background_executor().timer(Duration::from_secs(1)).await;
                    }
                }
                return Err(error);
            }
            Ok(serde_json::json!({"task":task_id,"node_id":node,"dispatch_id":dispatch,"status":"starting"}).to_string())
        }))
    }

    fn check_staff_source(
        &self,
        token: &str,
        scope: &norma::protocol::BrainScope,
    ) -> Result<(), String> {
        if self.brain.token_matches(token) && self.brain_scope().as_ref() == Some(scope) {
            Ok(())
        } else {
            Err("staff.invalid_source".to_owned())
        }
    }

    fn reserve_staff_worker(
        &mut self,
        proposal: &StaffProposal,
        prompt: String,
        cx: &mut Context<Self>,
    ) -> Result<(Worker, String), String> {
        let entry = self
            .agent_roster_for_provider()
            .into_iter()
            .find(|entry| entry.name == proposal.agent && entry.enabled)
            .ok_or_else(|| "staff.agent_unavailable".to_owned())?;
        let area = self.mobile_canvas_area(None);
        let zoom = self.runtime.document.viewport.zoom;
        let bounds = self.next_tiled_node_bounds_for_area(area, zoom);
        let dispatch = uuid::Uuid::new_v4().to_string();
        let profile =
            crate::app::agent_runtime::command_support::canvas_agent_profile(entry.profile);
        let node =
            self.runtime
                .create_staff_worker(profile, entry.name.clone(), bounds, dispatch.clone());
        self.apply_layout_for_new_node_for_area(node, area, zoom);
        self.notify_staff(cx);
        Ok((
            Worker::new(
                &entry.name,
                &format!("{:?}", entry.profile),
                &proposal.title,
                node,
                &dispatch,
            ),
            prompt,
        ))
    }

    pub(crate) fn approve_staff_proposal(
        &mut self,
        task_id: String,
        proposal_id: u64,
        cx: &mut Context<Self>,
    ) {
        if self.staff.busy {
            return;
        }
        let prepared = self.staff_store().and_then(|store| {
            let scope = self
                .brain_scope()
                .ok_or_else(|| "staff.invalid_source".to_owned())?;
            let token = self
                .brain
                .client
                .as_ref()
                .and_then(|client| client.token())
                .ok_or_else(|| "staff.invalid_source".to_owned())?;
            Ok((store, scope, token))
        });
        let (store, scope, token) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                self.staff.error = Some(crate::app::staff::error_message(error));
                self.notify_staff(cx);
                return;
            }
        };
        self.staff.busy = true;
        self.staff.error = None;
        let work = cx.background_executor().spawn(async move {
            store
                .approve(&scope.workspace_id, &task_id, proposal_id)
                .map_err(|error| error.to_string())?;
            let paths = norma::orchestration::InstallPaths::for_current_process()
                .map_err(|error| error.to_string())?;
            let result = std::process::Command::new(paths.bundled_cli)
                .args([
                    "brain",
                    "--workspace",
                    &scope.workspace_id,
                    "--route",
                    &scope.route_id,
                    "assign",
                    "--task",
                    &task_id,
                    "--proposal",
                    &proposal_id.to_string(),
                ])
                .envs(config::adapter_env(
                    &token,
                    &scope.workspace_id,
                    &scope.route_id,
                ))
                .output()
                .map_err(|error| error.to_string())?;
            if result.status.success() {
                Ok(())
            } else {
                Err(String::from_utf8_lossy(&result.stderr).into_owned())
            }
        });
        cx.spawn(async move |app, cx| {
            let result = work.await;
            let _ = app.update(cx, |app, cx| {
                app.staff.busy = false;
                app.staff.error = result.err().map(crate::app::staff::error_message);
                app.staff.loaded = false;
                app.load_staff_tasks(cx);
                app.notify_staff(cx);
            });
        })
        .detach();
        self.notify_staff(cx);
    }
}
