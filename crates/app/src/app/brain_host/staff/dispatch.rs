use super::*;
use agent::context::ContextOperation;
use norma::protocol::{StaffProposal, Worker};

/// 新开的卡 `card` 是 None。复用时记下改绑前的编号，绑失败就改回去；用户自己开的卡没有旧编号。
struct ReservedCard {
    card: Option<Option<String>>,
}

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
            if proposal.target_worker.is_some() {
                return super::resume::resume_staff_worker(
                    &entity,
                    cx,
                    store,
                    context,
                    (scope, token),
                    (task, proposal),
                )
                .await;
            }
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
            let (worker, prompt, reused) = reserved;
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
                    let _ = entity.update(cx, |app, cx| {
                        app.release_reserved_card(node, &reused);
                        app.notify_staff(cx);
                    });
                    return Err(error.to_string());
                }
            };
            let prepared = entity.update(cx, |app, cx| {
                app.check_staff_source(&token, &scope)?;
                app.receive_staff_tasks(vec![bound], cx);
                if reused.card.is_some() {
                    app.brain.own_prompts.remember(&prompt);
                    app.arm_staff_delivery(node, dispatch.clone(), prompt.clone(), true);
                    app.tick_staff_workers(Instant::now(), cx);
                    app.mark_canvas_dirty();
                    app.mark_session_dirty();
                    app.notify_staff(cx);
                    return Ok(None);
                }
                // Recheck effective restrictions immediately before accepting the launch.
                context.execute_with_state(ContextOperation::DispatchTask, None, |_| {
                    app.runtime.prepare_native_agent_terminal(node, 80, 24)
                        .map_err(|error| agent::AdapterError::Failed(error.to_string()))
                }).map(Some).map_err(|error| error.to_string())
            }).map_err(|_| "staff.invalid_source".to_owned()).and_then(|result| result);
            let result = match prepared {
                Ok(None) => {
                    return Ok(serde_json::json!({"task":task_id,"node_id":node,"dispatch_id":dispatch,"status":"starting"}).to_string());
                }
                Ok(Some(launch)) => {
                    let run = launch.run_id().to_owned();
                    let spawned = cx.background_executor().spawn(async move { launch.spawn() }).await;
                    entity.update(cx, |app, cx| {
                        app.check_staff_source(&token, &scope)?;
                        let live = spawned.and_then(|prepared| prepared.into_live());
                        app.runtime.complete_native_agent_terminal(node, &run, live).map_err(|error| error.to_string())?;
                        app.brain.own_prompts.remember(&prompt);
                        app.arm_staff_delivery(node, dispatch.clone(), prompt, false);
                        app.tick_staff_workers(Instant::now(), cx);
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
                    app.staff.clocks.remove(&node);
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

    pub(super) fn check_staff_source(
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
    ) -> Result<(Worker, String, ReservedCard), String> {
        let entry = self.resolve_staff_entry(&proposal.agent)?;
        let dispatch = uuid::Uuid::new_v4().to_string();
        let named = self.roster_entry_if_named_target(&proposal.agent).is_some();
        let (node, reused) = if named {
            if let Some(existing) = self.reusable_named_node(&entry.name) {
                let previous = self.runtime.staff_dispatch_id(existing).map(str::to_owned);
                self.runtime
                    .rebind_staff_dispatch(existing, dispatch.clone());
                (
                    existing,
                    ReservedCard {
                        card: Some(previous),
                    },
                )
            } else {
                (
                    self.open_staff_card(&entry, &dispatch)?,
                    ReservedCard { card: None },
                )
            }
        } else {
            (
                self.open_staff_card(&entry, &dispatch)?,
                ReservedCard { card: None },
            )
        };
        self.notify_staff(cx);
        let mut worker = Worker::new(
            &entry.name,
            &format!("{:?}", entry.profile),
            &proposal.title,
            node,
            &dispatch,
        );
        worker.launch_prompt = prompt.clone();
        worker.reused = reused.card.is_some();
        if let Some(model) = proposal.model.clone() {
            self.runtime.set_staff_model(node, model);
        }
        Ok((worker, prompt, reused))
    }

    fn open_staff_card(
        &mut self,
        entry: &agent::AgentRosterEntry,
        dispatch: &str,
    ) -> Result<u64, String> {
        let area = self.mobile_canvas_area(None);
        let zoom = self.runtime.document.viewport.zoom;
        let bounds = self.next_tiled_node_bounds_for_area(area, zoom);
        let profile =
            crate::app::agent_runtime::command_support::canvas_agent_profile(entry.profile);
        let node = self.runtime.create_staff_worker(
            profile,
            entry.name.clone(),
            bounds,
            dispatch.to_owned(),
        );
        self.apply_layout_for_new_node_for_area(node, area, zoom);
        Ok(node)
    }

    /// 点名时找同名空闲卡。进程已经退出的卡不算空闲。
    fn reusable_named_node(&self, name: &str) -> Option<u64> {
        let cards: Vec<pick::CardCandidate> = self
            .runtime
            .document
            .nodes()
            .iter()
            .filter_map(|node| {
                let canvas::CanvasNodeKind::Agent(spec) = &node.kind else {
                    return None;
                };
                let alias = spec.alias.clone()?;
                let busy = matches!(
                    self.runtime.agent_task_state(node.id),
                    Some(
                        runtime::AgentTaskState::Thinking
                            | runtime::AgentTaskState::RunningTool { .. }
                            | runtime::AgentTaskState::WaitingApproval { .. }
                            | runtime::AgentTaskState::WaitingQuestion
                    )
                ) || self.runtime.terminal_has_exited(node.id);
                Some(pick::CardCandidate {
                    node: node.id,
                    name: alias,
                    busy,
                    user_opened: self.runtime.staff_spawn_id(node.id).is_none(),
                    claimable: self.runtime.staff_card_claimable(node.id),
                })
            })
            .collect();
        pick::reusable_card(name, &cards).map(|card| card.node)
    }

    fn release_reserved_card(&mut self, node: u64, reused: &ReservedCard) {
        match &reused.card {
            None => {
                self.runtime.remove_node(node);
            }
            Some(Some(previous)) => self.runtime.rebind_staff_dispatch(node, previous.clone()),
            Some(None) => self.runtime.clear_staff_dispatch(node),
        }
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
        let (agent, model) = self.staff_proposal_choice(&task_id, proposal_id);
        self.staff.busy = true;
        self.staff.error = None;
        let work = cx.background_executor().spawn(async move {
            store
                .approve(&scope.workspace_id, &task_id, proposal_id, agent, model)
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
