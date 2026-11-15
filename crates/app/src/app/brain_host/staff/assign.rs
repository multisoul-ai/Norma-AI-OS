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
        // 设置里的「后续任务」为「自动」时幕僚才可能直接派发。
        let auto = self.brain_autonomy().assign_followups;
        let task_id = op.task_id().unwrap_or_default().to_owned();
        let workspace = scope.workspace_id.clone();
        let work = cx
            .background_executor()
            .spawn(async move { store.assign(&workspace, op, auto) });
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
            .workers
            .iter()
            .find(|w| w.dispatch_id == dispatch && w.outcome.is_none())
            .and_then(|w| w.card_id)
        else {
            return;
        };
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
        self.runtime.remove_node(node);
        self.mark_canvas_dirty();
        self.mark_session_dirty();
        self.notify_staff(cx);
    }
}
