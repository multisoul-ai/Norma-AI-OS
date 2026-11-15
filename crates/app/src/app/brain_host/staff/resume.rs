//! 续派：把已批准的提议交给它指向的已有 Worker，在同一张卡片里继续，不新建卡片。
use super::*;
use crate::app::shared_context::SharedContext;
use crate::app::staff::{DeliveryStage, PendingWorker};
use agent::context::ContextOperation;
use norma::protocol::{BrainScope, StaffProposal, Task};

/// 卡片被关掉或进程已退出时 prompt 没处可投；报错，由幕僚改用新建并走确认。
pub(super) async fn resume_staff_worker(
    entity: &gpui::WeakEntity<MultisoulApp>,
    cx: &mut gpui::AsyncApp,
    store: std::sync::Arc<brain::staff::TaskStore>,
    context: SharedContext,
    (scope, token): (BrainScope, String),
    (task, proposal): (Task, StaffProposal),
) -> Result<String, String> {
    let target = proposal.target_worker.clone().unwrap_or_default();
    let node = task
        .worker_for_dispatch(&target)
        .and_then(|worker| worker.card_id)
        .ok_or_else(|| "staff.worker_card_gone".to_owned())?;
    let handoff = proposal.prompt.clone();
    let prompt = entity
        .update(cx, |app, _| {
            app.check_staff_source(&token, &scope)?;
            if app.runtime.document.node(node).is_none() || app.runtime.terminal_has_exited(node) {
                return Err("staff.worker_card_gone".to_owned());
            }
            context
                .execute_with_state(ContextOperation::DispatchTask, None, |state| {
                    agent::context::context_task_handoff(
                        &handoff,
                        &[("source_and_destination", state)],
                    )
                    .map_err(|reason| agent::AdapterError::Rejected(reason.into()))
                })
                .map_err(|error| error.to_string())
        })
        .map_err(|_| "staff.invalid_source".to_owned())??;
    let (workspace, id, proposal_id) = (scope.workspace_id.clone(), task.id.clone(), proposal.id);
    let writer = store.clone();
    let stored = prompt.clone();
    let rebound = cx
        .background_executor()
        .spawn(async move { writer.rebind_worker(&workspace, &id, proposal_id, &stored) })
        .await
        .map_err(|error| error.to_string())?;
    entity
        .update(cx, |app, cx| {
            app.check_staff_source(&token, &scope)?;
            app.receive_staff_tasks(vec![rebound], cx);
            app.brain.own_prompts.remember(&prompt);
            // 进程早已在运行：不必再等启动宽限，卡片接受输入就直接投递。
            app.staff.pending.insert(
                node,
                PendingWorker {
                    dispatch: target.clone(),
                    prompt,
                    stage: DeliveryStage::Starting {
                        since: Instant::now(),
                        grace_spent: true,
                    },
                },
            );
            app.tick_staff_workers(Instant::now(), cx);
            app.mark_session_dirty();
            app.notify_staff(cx);
            Ok::<(), String>(())
        })
        .map_err(|_| "staff.invalid_source".to_owned())??;
    Ok(serde_json::json!({"task": task.id, "node_id": node, "dispatch_id": target, "status": "continued"}).to_string())
}
