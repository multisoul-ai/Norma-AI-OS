//! 幕僚自己派活：后台按策略落库，满足条件才复用用户采用时的同一条派发链路。
use super::*;
use brain::staff::AssignOutcome;

impl MultisoulApp {
    pub(super) fn assign_staff_worker(
        &mut self,
        op: StaffOp,
        token: String,
        cx: &mut Context<Self>,
    ) -> Result<gpui::Task<Result<String, String>>, String> {
        let store = self.staff_store()?;
        let scope = self
            .brain_scope()
            .ok_or_else(|| "island.staff_no_workspace".to_owned())?;
        // 幕僚开关。主会话的 assign_followups 不在这里读。
        let auto = self.brain_autonomy().staff_auto_dispatch;
        let task_id = op.task_id().unwrap_or_default().to_owned();
        let agent = match &op {
            StaffOp::Assign { agent, .. } => agent.clone(),
            _ => String::new(),
        };
        let named_person = self.roster_entry_if_named_target(&agent);
        let workspace = scope.workspace_id.clone();
        let words_task = task_id.clone();
        let work = cx.background_executor().spawn(async move {
            let named = named_person
                .as_ref()
                .and_then(|entry| {
                    let words = store.get(&workspace, &words_task).ok()?.user_words;
                    Some(brain::staff::naming::named_in(
                        &words,
                        brain::staff::naming::NameRef {
                            name: &entry.name,
                            speech_alias: entry.speech_alias.as_deref(),
                        },
                    ))
                })
                .unwrap_or(false);
            let identity = named_person
                .as_ref()
                .map(|entry| (entry.definition_id, format!("{:?}", entry.profile)));
            store.assign_for_recipient(
                &workspace,
                op,
                auto,
                named,
                identity.as_ref().map(|(id, engine)| (*id, engine.as_str())),
            )
        });
        Ok(cx.spawn(async move |app, cx| {
            let (task, outcome) = work.await.map_err(|error| error.to_string())?;
            let work = app
                .update(cx, |app, cx| {
                    if app.brain_scope().as_ref() != Some(&scope) {
                        return Err("staff.invalid_source".to_owned());
                    }
                    app.receive_staff_tasks(vec![task], cx);
                    match outcome {
                        AssignOutcome::Dispatch { proposal_id } => {
                            app.dispatch_staff_worker(task_id, proposal_id, token, cx)
                        }
                        AssignOutcome::Proposed {
                            proposal_id,
                            reason,
                        } => Ok(gpui::Task::ready(Ok(serde_json::json!({
                            "task": task_id,
                            "status": "proposed",
                            "proposal_id": proposal_id,
                            "reason": reason.code(),
                        })
                        .to_string()))),
                    }
                })
                .map_err(|_| "staff.invalid_source".to_owned())??;
            work.await
        }))
    }
}

impl MultisoulApp {
    /// 用户撤回幕僚自动派发的活：记为被中断并关闭那张 Agent 卡。已有结果的 Worker 不处理。
    pub(crate) fn cancel_staff_worker(
        &mut self,
        task_id: &str,
        proposal_id: u64,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let Some((dispatch, node, effects)) = self.staff.tasks.get(task_id).and_then(|task| {
            let proposal = task.proposals.iter().find(|p| p.id == proposal_id)?;
            let dispatch = proposal.dispatch_id.clone()?;
            let worker = task.worker_for_dispatch(&dispatch)?;
            let node = worker.card_id?;
            let (next, effects) = brain::staff::worker_life::step(
                worker.phase,
                brain::staff::worker_life::Input::Withdraw {
                    close_card: !worker.reused,
                },
            );
            // 已经跑起来，或这一轮已经结束：撤回不再处理。
            (next != worker.phase).then_some((dispatch, node, effects))
        }) else {
            return;
        };
        if self.runtime.document.node(node).is_none() {
            return;
        }
        self.staff.clocks.remove(&node);
        for effect in effects {
            match effect {
                brain::staff::worker_life::Effect::Interrupt => {
                    self.interrupt_agent_by_user(node);
                }
                brain::staff::worker_life::Effect::CloseCard => {
                    self.remove_node_and_reflow(node, window);
                }
                _ => {}
            }
        }
        self.push_brain(BrainEventKind::WorkerWithdrawn {
            node_id: node,
            dispatch_id: dispatch,
        });
        self.mark_session_dirty();
        self.notify_staff(cx);
    }
}
