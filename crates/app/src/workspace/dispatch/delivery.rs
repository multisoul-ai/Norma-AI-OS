use super::*;
use crate::app::MultisoulApp;
use agent::card_instance::{
    CardInstanceAssignment, CardInstanceEvent, CardInstanceFailure, CardInstanceStage,
    CardInstanceTracking,
};
use gpui::Entity;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use storage::{
    NewWorkspaceOperation, WorkspaceAssignmentKind, WorkspaceOperationAssignment,
    WorkspaceOperationAuthorization,
};

pub(super) async fn submit(
    database: PathBuf,
    pending: PendingWorkspaceDispatch,
    plan: WorkspaceTaskPlan,
    hook_paths: norma::agent_hook::HookInstallPaths,
    cx: &mut gpui::AsyncApp,
) -> Result<WorkspaceOperationReceipt, String> {
    agent::workspace_plan::validate_workspace_task_plan(
        &pending.manifest,
        &pending.resolution,
        &pending.targets,
    )
    .map_err(|error| format!("invalid_original_plan:{error:?}"))?;
    if plan.tasks().len() != pending.targets.len()
        || plan
            .tasks()
            .iter()
            .zip(&pending.targets)
            .any(|(task, target)| task.card() != &target.card)
    {
        return Err("workspace_planning_target_set_mismatch".into());
    }
    let path = database.clone();
    let source = pending.entry.source.clone();
    let fingerprint = pending.fingerprint.clone();
    if let Some(receipt) = cx
        .background_executor()
        .spawn(async move {
            WorkspaceOperationStore::new(path)
                .lookup_original_request(&source, &fingerprint)
                .map_err(|e| e.to_string())
        })
        .await?
    {
        return Ok(receipt);
    }
    let mut targets = pending.targets;
    let mut sessions = HashMap::new();
    let mut tracking = HashMap::new();
    let path = database.clone();
    let workspace = pending.entry.source.workspace_id.clone();
    let areas = cx
        .background_executor()
        .spawn(async move {
            storage::DesktopWorkspaceStore::new(path)
                .list_projects(&workspace)
                .map_err(|e| e.to_string())
        })
        .await?;
    for target in &mut targets {
        let mut current = read_directory(&database, &pending.entry.source.workspace_id, cx).await?;
        cx.update(|cx| {
            super::super::directory::project_retained_directory(&database, &mut current, cx)
        })
        .map_err(|e| e.to_string())?;
        runtime::workspace_resolution::revalidate_workspace_binding(&current, target)
            .map_err(|e| format!("target_changed:{e:?}"))?;
        let area = areas
            .iter()
            .find(|area| {
                area.id == target.card.area_id
                    && area.project_key.to_string_lossy() == target.project.project_id
            })
            .ok_or("project_changed")?
            .clone();
        let task = cx
            .update(|cx| super::super::desktop::mobile::ensure(&database, area, cx))
            .map_err(|e| e.to_string())?;
        let session = task.await?;
        let (execution, managed) =
            super::preparation::prepare(&session, target, &hook_paths, cx).await?;
        tracking.insert(target.card.clone(), managed);
        // Only cold targets capture their first execution identity, never refresh an accepted one.
        target.execution = Some(execution);
        sessions.insert(target.card.area_id.clone(), session);
    }
    let mut current = read_directory(&database, &pending.entry.source.workspace_id, cx).await?;
    cx.update(|cx| {
        super::super::directory::project_retained_directory(&database, &mut current, cx)
    })
    .map_err(|e| e.to_string())?;
    for target in &targets {
        runtime::workspace_resolution::revalidate_workspace_binding(&current, target)
            .map_err(|e| format!("target_changed:{e:?}"))?;
    }
    let source = pending.entry.source;
    let assignments = cx
        .update(|cx| {
            plan.into_tasks()
                .into_iter()
                .zip(targets)
                .enumerate()
                .map(|(index, (task, target))| {
                    let session = sessions
                        .get(&target.card.area_id)
                        .ok_or("session_missing")?;
                    let app = session.read(cx);
                    let definition_id =
                        app.runtime
                            .document
                            .node(target.card.node_id)
                            .and_then(|n| match &n.kind {
                                canvas::CanvasNodeKind::Agent(s) => s.definition_id,
                                _ => None,
                            });
                    let tracking = if tracking.get(&target.card).copied().unwrap_or(false) {
                        CardInstanceTracking::Managed
                    } else {
                        CardInstanceTracking::Untracked
                    };
                    let role = current
                        .roles
                        .iter()
                        .find(|r| r.card == target.card)
                        .ok_or("target_missing")?;
                    let project = current
                        .projects
                        .iter()
                        .find(|p| p.reference == target.project)
                        .ok_or("project_missing")?;
                    let area = current
                        .areas
                        .iter()
                        .find(|a| a.area_id == target.card.area_id)
                        .ok_or("area_missing")?;
                    let id = format!("{}:{}:{}", source.origin.key(), source.operation_id, index);
                    let assignment = CardInstanceAssignment {
                        assignment_id: id.clone(),
                        target: WorkspaceCardTarget {
                            card: target.card,
                            execution: target.execution.ok_or("execution_missing")?,
                        },
                        name_at_acceptance: role.name.clone(),
                        definition_id,
                        text: task.into_parts().1,
                        tracking,
                        force_untracked: false,
                    };
                    app.preflight_card_instance_assignment(
                        &source.workspace_id,
                        &area.area_id,
                        assignment.target.execution.route_generation,
                        &assignment,
                    )
                    .map_err(|e| e.code)?;
                    Ok(WorkspaceOperationAssignment {
                        assignment,
                        kind: WorkspaceAssignmentKind::Task,
                        task_ref: id.clone(),
                        logical_task_ref: id,
                        attempt_sequence: 1,
                        project_id: project.reference.project_id.clone(),
                        project_name: project.name.clone(),
                        area_name: area.name.clone(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .map_err(|e| e.to_string())??;
    let request = NewWorkspaceOperation {
        source,
        original_request_fingerprint: pending.fingerprint,
        assignments,
        expected_role_revision: current.revision,
        authorization: WorkspaceOperationAuthorization::Desktop,
    };
    // Register before awaiting the acceptance transaction, so role mutations cannot miss
    // the window between durable reception and installation of the foreground queue.
    let authorities: HashMap<_, _> = request
        .assignments
        .iter()
        .map(|item| {
            (
                item.assignment.assignment_id.clone(),
                super::authority::register(&database, &item.assignment.target.card),
            )
        })
        .collect();
    if authorities
        .values()
        .any(|authority| !authority.load(Ordering::Acquire))
    {
        return Err("authorization_changed".into());
    }
    let path = database.clone();
    let journal = shared_journal(&database, cx).await?;
    let accepted = cx
        .background_executor()
        .spawn(async move {
            let store = WorkspaceOperationStore::new(path);
            store.accept(&request).map_err(|e| e.to_string())
        })
        .await?;
    if !accepted.newly_accepted {
        return Ok(accepted.receipt);
    }
    for item in &accepted.receipt.assignments {
        journal.watch(
            &item.assignment.assignment_id,
            authorities[&item.assignment.assignment_id].clone(),
        );
    }
    for (index, item) in accepted.receipt.assignments.iter().enumerate() {
        let authority = authorities[&item.assignment.assignment_id].clone();
        if let Err(code) = deliver(&database, item, &sessions, &journal, authority, cx).await {
            for authority in authorities.values() {
                authority.store(false, Ordering::Release);
            }
            // None of these items has been installed. Preserve zero-input failures for the
            // remainder as well; transient DB errors must not leave an accepted group stranded.
            for pending in &accepted.receipt.assignments[index..] {
                record_failure(pending, &journal, &code);
            }
            break;
        }
    }
    let store = WorkspaceOperationStore::new(database);
    let fallback = accepted.receipt;
    let source = fallback.source.clone();
    cx.background_executor()
        .spawn(async move {
            store
                .receipt(&source)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "receipt_missing".to_owned())
        })
        .await
        .or(Ok(fallback))
}

async fn deliver(
    database: &std::path::Path,
    item: &storage::StoredWorkspaceAssignment,
    sessions: &HashMap<String, Entity<MultisoulApp>>,
    journal: &super::journal::WorkspaceJournal,
    authority: Arc<AtomicBool>,
    cx: &mut gpui::AsyncApp,
) -> Result<(), String> {
    let session = sessions
        .get(&item.assignment.target.card.area_id)
        .ok_or("session_missing")?;
    let item = item.clone();
    let path = database.to_owned();
    let id = item.assignment.assignment_id.clone();
    let store = WorkspaceOperationStore::new(path);
    let arm = cx
        .background_executor()
        .spawn(async move { store.arm_input(&id).map_err(|e| e.to_string()) })
        .await;
    if !arm? {
        return Ok(());
    }
    let mut armed = item.clone();
    armed.input_phase = storage::WorkspaceInputPhase::Armed;
    let result = session
        .update(cx, |app, _| {
            app.accept_persisted_card_instance_assignment(
                &armed,
                authority.clone(),
                journal.clone(),
            )
        })
        .map_err(|e| e.to_string())
        .and_then(|r| r.map_err(|e| e.code));
    result.map(|_| ())
}

fn record_failure(
    item: &storage::StoredWorkspaceAssignment,
    journal: &super::journal::WorkspaceJournal,
    code: &str,
) {
    let mut receipt = item.receipt.clone();
    receipt.stage = CardInstanceStage::Failed;
    receipt.waiting_reason = None;
    receipt.failure = Some(CardInstanceFailure {
        code: code.into(),
        retry_safe: true,
    });
    let event = CardInstanceEvent {
        receipt,
        evidence: code.into(),
        observed_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(i64::MAX as u128) as u64,
    };
    journal.enqueue(event);
}
