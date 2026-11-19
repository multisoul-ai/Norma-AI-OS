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
        let named_person = self
            .roster_entry_if_named_target(&agent)
            .map(|entry| (entry.name, entry.speech_alias));
        let workspace = scope.workspace_id.clone();
        let words_task = task_id.clone();
        let work = cx.background_executor().spawn(async move {
            let named = named_person
                .as_ref()
                .and_then(|(name, alias)| {
                    let words = store.get(&workspace, &words_task).ok()?.user_words;
                    Some(brain::staff::naming::named_in(
                        &words,
                        brain::staff::naming::NameRef {
                            name,
                            speech_alias: alias.as_deref(),
                        },
                    ))
                })
                .unwrap_or(false);
            store.assign(&workspace, op, auto, named)
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
        let Some(task) = self.staff.tasks.get(task_id) else {
            return;
        };
        let Some(proposal) = task.proposals.iter().find(|p| p.id == proposal_id) else {
            return;
        };
        let Some(dispatch) = proposal.dispatch_id.as_deref() else {
            return;
        };
        let Some(node) = task
            .worker_for_dispatch(dispatch)
            .filter(|w| w.outcome.is_none())
            .and_then(|w| w.card_id)
        else {
            return;
        };
        // 连点或卡片已被关掉：不再推第二条空事件。
        if self.runtime.document.node(node).is_none() {
            return;
        }
        let agent = self.agent_announcement_name(node).unwrap_or_default();
        let dispatch_id = self.runtime.staff_dispatch_id(node).map(str::to_owned);
        self.staff.pending.remove(&node);
        self.push_brain(BrainEventKind::AgentFinished {
            node_id: node,
            dispatch_id,
            agent,
            outcome: Outcome::Interrupted,
            excerpt: "staff.worker_cancelled".into(),
            from_result: true,
        });
        self.remove_node_and_reflow(node, window);
        self.mark_session_dirty();
        self.notify_staff(cx);
    }
}
