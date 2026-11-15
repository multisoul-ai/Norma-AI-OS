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
            let dispatch = app
                .update(cx, |app, cx| {
                    if app.brain_scope().as_ref() != Some(&scope) {
                        return Err("staff.invalid_source".to_owned());
                    }
                    app.receive_staff_tasks(vec![task], cx);
                    match &outcome {
                        AssignOutcome::Dispatch { proposal_id } => app
                            .dispatch_staff_worker(task_id.clone(), *proposal_id, token, cx)
                            .map(Some),
                        AssignOutcome::Proposed { .. } => Ok(None),
                    }
                })
                .map_err(|_| "staff.invalid_source".to_owned())??;
            match (dispatch, outcome) {
                (Some(dispatch), _) => dispatch.await,
                (
                    None,
                    AssignOutcome::Proposed {
                        proposal_id,
                        reason,
                    },
                ) => Ok(serde_json::json!({
                    "task": task_id,
                    "status": "proposed",
                    "proposal_id": proposal_id,
                    "reason": reason.code(),
                })
                .to_string()),
                (None, AssignOutcome::Dispatch { .. }) => unreachable!("dispatch yields a task"),
            }
        }))
    }
}
