use super::WorkspaceTarget;
use crate::{
    app::MultisoulApp,
    mobile::{MobileCommand, MobileCommandRequest},
};
use gpui::{AsyncApp, Context};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub(super) fn start(
    app: &mut MultisoulApp,
    target: WorkspaceTarget,
    database: PathBuf,
    request: MobileCommandRequest,
    cx: &mut Context<MultisoulApp>,
) -> Result<async_channel::Receiver<Result<Value, &'static str>>, &'static str> {
    let MobileCommand::GeneratePlan {
        direction,
        priority,
    } = request.command.clone()
    else {
        return Err("invalid_plan_input");
    };
    let config = app
        .voice_settings
        .values()
        .agent_provider_config()
        .map_err(|_| "provider_unconfigured")?;
    let cards = app.runtime.mobile_card_snapshots();
    let environment = app
        .current_canvas_context_for_provider()
        .ok_or("route_expired")?;
    let context = agent::render_norma_environment(&environment, &[]);
    let (reply, result) = async_channel::bounded(1);
    let source_database = database.clone();
    let workspace = request.workspace_id.clone();
    let route = request.route_id.clone();
    // Construction and the network future run off GPUI; no credentials enter the phone protocol.
    let work = cx.background_executor().spawn(async move {
        let source = if let Some(priority) = &priority {
            let store = storage::ProjectPlanStore::new(source_database);
            let plan = store
                .get(&workspace, &priority.plan_id)
                .map_err(|_| "unavailable")?
                .ok_or("plan_changed")?;
            let latest = store.list(&workspace, None, 1).map_err(|_| "unavailable")?;
            if plan.route_id != route
                || latest.items.first().map(|p| &p.plan_id) != Some(&plan.plan_id)
            {
                return Err("plan_changed");
            }
            Some(plan)
        } else {
            None
        };
        let priority_todo = if let (Some(plan), Some(priority)) = (&source, &priority) {
            let todo = plan
                .draft
                .todos
                .get(priority.todo_index)
                .ok_or("invalid_plan_priority")?;
            if !cards.iter().any(|card| {
                card.node_id == todo.target.node_id
                    && card.profile == todo.target.profile
                    && card.mobile_control
                    && card.run_id == todo.target.run_id
                    && card.session_id == todo.target.session_id
            }) {
                return Err("priority_card_changed");
            }
            Some(todo.clone())
        } else {
            None
        };
        let runtime = crate::app::shared_tokio_runtime().map_err(|_| "unavailable")?;
        let input = direction.clone();
        let task = runtime.spawn(async move {
            agent::generate_project_plan(&config, &input, &context, &cards, priority_todo.as_ref())
                .await
        });
        Ok::<_, &'static str>((task, direction, source))
    });
    cx.spawn(async move |_, cx| {
        let (task, direction, source) = match work.await {
            Ok(value) => value,
            Err(error) => {
                let _ = reply.try_send(Err(error));
                return;
            }
        };
        // 25s leaves 5s for the existing 30s command receipt. Closing cancels local inference.
        let deadline = Instant::now() + Duration::from_secs(25);
        while !task.is_finished() {
            if !live(&target, &database, &request, None, cx) || Instant::now() >= deadline {
                task.abort();
                let _ = reply.try_send(Err("route_expired"));
                return;
            }
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
        }
        let draft = match task.await {
            Ok(Ok(draft)) => draft,
            Ok(Err(error)) => {
                let _ = reply.try_send(Err(error));
                return;
            }
            Err(_) => {
                let _ = reply.try_send(Err("plan_provider_failed"));
                return;
            }
        };
        if !live(&target, &database, &request, Some(&draft), cx) {
            let _ = reply.try_send(Err("session_changed"));
            return;
        }
        let outcome = cx
            .background_executor()
            .spawn(async move {
                let id = super::new_workspace_route_id().map_err(|_| "unavailable")?;
                let store = storage::ProjectPlanStore::new(database);
                let plan = if let Some(source) = source {
                    store.publish_after(&id, &source, &direction, &draft)
                } else {
                    store.publish(
                        &id,
                        &request.workspace_id,
                        &request.route_id,
                        &direction,
                        &draft,
                    )
                }
                .map_err(|error| match error {
                    storage::StorageError::InvalidProjectPlan(code) if code == "plan_changed" => {
                        "plan_changed"
                    }
                    _ => "plan_persistence_failed",
                })?;
                Ok(json!({"plan_id":plan.plan_id,"status":"proposed"}))
            })
            .await;
        let _ = reply.try_send(outcome);
    })
    .detach();
    Ok(result)
}

fn live(
    target: &WorkspaceTarget,
    database: &std::path::Path,
    request: &MobileCommandRequest,
    draft: Option<&agent::ProjectPlanDraft>,
    cx: &mut AsyncApp,
) -> bool {
    target
        .app
        .update(cx, |app, _| {
            if app.database_paths.app_db_path() != database
                || app.workspace_id.as_deref() != Some(&request.workspace_id)
                || app.workspace_route_id.as_deref() != Some(&request.route_id)
            {
                return false;
            }
            let Some(draft) = draft else {
                return true;
            };
            let current = app.runtime.mobile_card_snapshots();
            draft.todos.iter().all(|todo| {
                current.iter().any(|card| {
                    card.node_id == todo.target.node_id
                        && card.profile == todo.target.profile
                        && card.mobile_control
                        && card.run_id == todo.target.run_id
                        && card.session_id == todo.target.session_id
                })
            })
        })
        .ok()
        .unwrap_or(false)
}
