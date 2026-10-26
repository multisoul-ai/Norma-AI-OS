use super::*;
use storage::{AgentDispatchTaskState as State, MobileTurnKey, MobileTurnState};

fn state_name(state: State) -> &'static str {
    match state {
        State::Done => "succeeded",
        State::Failed => "failed",
        State::Interrupted => "interrupted",
        State::Cancelled => "cancelled",
        State::CancelRequested => "cancel_requested",
        State::Accepted => "accepted",
        State::Submitted => "submitted",
        State::Queued => "queued",
        State::Untracked => "untracked",
    }
}

pub(super) fn task_payload(
    task: &storage::StoredAgentDispatchTask,
    target: &WorkspaceAgentTarget,
) -> Option<Value> {
    if !matches!(
        task.state,
        State::Done | State::Failed | State::Interrupted | State::Cancelled
    ) {
        return None;
    }
    if task.node_id != target.node_id || task.definition_id != target.definition_id {
        return None;
    }
    Some(
        json!({"sender":"Agent","text":task.result.as_ref().and_then(|r|r.text.clone()).unwrap_or_else(||format!("任务{}；Mac 未提供正文结果。",state_name(task.state))),"target":target,"task_ref":task.task_ref,"runtime_instance_id":task.runtime_instance_id,"truncated":task.result.as_ref().is_some_and(|r|r.truncated),"error":task.error,"state":state_name(task.state)}),
    )
}

pub(super) fn monitor_result(
    database: PathBuf,
    device: String,
    operation: String,
    session: Entity<MultisoulApp>,
    mut target: WorkspaceAgentTarget,
    result: AdapterResult,
    cx: &mut App,
) -> Result<async_channel::Receiver<Result<Value, String>>, String> {
    let (reply, receiver) = async_channel::bounded(1);
    let task = result
        .task_id
        .and_then(|id| {
            session
                .read(cx)
                .agent_task_coordinator
                .as_ref()?
                .tasks()
                .into_iter()
                .find(|t| t.task_id == id)
        })
        .ok_or("outcome_unknown")?;
    target.task_ref = Some(task.task_ref.clone());
    let writer = session
        .read(cx)
        .persistence
        .agent_dispatch_writer()
        .cloned()
        .ok_or("outcome_unknown")?;
    let project = session
        .read(cx)
        .persistence
        .project_store()
        .cloned()
        .ok_or("outcome_unknown")?;
    cx.spawn(async move|cx| {
        let store=storage::WorkspaceChatStore::new(database);
        let initial=json!({"sender":"Agent","text":result.message,"target":target,"task_ref":task.task_ref,"state":"submitted"});
        let event_store=store.clone();let event_device=device.clone();let event_op=operation.clone();let event=initial.to_string();
        let verify_writer=writer.clone();let verify_project=project.clone();let verify_task=task.task_ref.clone();let node=target.node_id;
        let verified=cx.background_executor().spawn(async move {
            if verify_writer.flush().is_err() || verify_project.agent_dispatch_task_store().get_by_task_ref(&verify_task).ok().flatten().is_none_or(|t| t.node_id != node) {return false;}
            event_store.record_event(&event_device,&event_op,"submitted",&event).is_ok()
        }).await;
        if !verified {let _=reply.try_send(Err("outcome_unknown".into()));return;}
        let mut binding=(target.run_id.clone(),target.session_id.clone());
        loop {
            let snapshot=session.read_with(cx,|app,_| app.mobile_card_snapshots().into_iter().find(|c|c.node_id==target.node_id)).ok().flatten();
            if target.run_id.is_none() {
                if let Some(card)=&snapshot {target.run_id=card.run_id.clone();target.session_id=card.session_id.clone();}
            } else if target.session_id.is_none() {
                if let Some(card)=&snapshot {if card.run_id==target.run_id {target.session_id=card.session_id.clone();}}
            }
            if binding!=(target.run_id.clone(),target.session_id.clone()) {
                binding=(target.run_id.clone(),target.session_id.clone());
                let bound=json!({"sender":"Agent","text":"任务已绑定真实运行","target":target,"task_ref":task.task_ref,"state":"running"}).to_string();
                let bound_store=store.clone();let bound_device=device.clone();let bound_operation=operation.clone();
                if cx.background_executor().spawn(async move{bound_store.record_event(&bound_device,&bound_operation,"running",&bound)}).await.is_err() {let _=reply.try_send(Err("outcome_unknown".into()));return;}
            }
            let query_project=project.clone();let query_writer=writer.clone();let reference=task.task_ref.clone();
            let stored=cx.background_executor().spawn(async move {
                query_writer.flush()?;
                query_project.agent_dispatch_task_store().get_by_task_ref(&reference)
            }).await;
            match stored {
                Ok(Some(stored))=> {
                    if let Some(payload)=task_payload(&stored,&target) {
                        let event_store=store.clone();let event_device=device.clone();let event_op=operation.clone();let body=payload.to_string();
                        let saved=cx.background_executor().spawn(async move{event_store.record_event(&event_device,&event_op,"task_result",&body)}).await;
                        let _=reply.try_send(if saved.is_ok(){Ok(payload)}else{Err("outcome_unknown".into())});return;
                    }
                }
                _=> {let _=reply.try_send(Err("outcome_unknown".into()));return;}
            }
            if snapshot.is_none() {let _=reply.try_send(Err("outcome_unknown".into()));return;}
            cx.background_executor().timer(Duration::from_secs(1)).await;
        }
    }).detach();
    Ok(receiver)
}

pub(super) fn handoff_reply(
    state: &agent::context::WorkingState,
    text: &str,
) -> Result<String, agent::AdapterError> {
    agent::context::context_task_handoff(text, &[("source_and_destination", state)])
        .map_err(|error| agent::AdapterError::Rejected(error.into()))
}

/// Replies use the existing card's acknowledged turn, never the previous task's completion.
pub(super) fn reply_to_card(
    database: PathBuf,
    device: String,
    operation: String,
    session: Entity<MultisoulApp>,
    mut target: WorkspaceAgentTarget,
    text: String,
    cx: &mut App,
) -> Result<async_channel::Receiver<Result<Value, String>>, String> {
    let card = session
        .read(cx)
        .mobile_card_snapshots()
        .into_iter()
        .find(|c| c.node_id == target.node_id)
        .ok_or("target_missing")?;
    let waiting = session.update(cx, |app, cx| {
        let service = app
            .shared_context_service()
            .map_err(|error| error.to_string())?;
        // The card receives the same goal/constraint handoff as desktop replies; the
        // operation id and permission checks stay keyed on the user's original text.
        service
            .execute_with_state(
                agent::context::ContextOperation::DispatchTask,
                None,
                |state| {
                    let text = handoff_reply(state, &text)?;
                    Ok(app.send_workspace_chat(&card, text, cx))
                },
            )
            .map_err(|error| error.to_string())?
            .map_err(str::to_owned)
    })?;
    let workspace = session
        .read(cx)
        .workspace_id
        .clone()
        .ok_or("project_unavailable")?;
    let (reply, receiver) = async_channel::bounded(1);
    cx.spawn(async move|cx| {
        let receipt=waiting.recv().await.unwrap_or(Err("outcome_unknown".into()));
        let receipt=match receipt {Ok(r)=>r,Err(e)=>{let _=reply.try_send(Err(e));return;}};
        target.run_id=receipt["run_id"].as_str().map(str::to_owned);
        target.session_id=receipt["session_id"].as_str().map(str::to_owned);
        let key=MobileTurnKey {node_id:target.node_id,profile:card.profile,run_id:target.run_id.clone().unwrap_or_default(),session_id:target.session_id.clone().unwrap_or_default(),turn_id:receipt["turn_id"].as_str().unwrap_or_default().into()};
        if key.validate().is_err() {let _=reply.try_send(Err("outcome_unknown".into()));return;}
        let store=storage::WorkspaceChatStore::new(database.clone());
        let event=json!({"sender":"Agent","text":"指定卡片已接收消息","target":target,"task_ref":target.task_ref,"turn_id":key.turn_id,"profile":key.profile,"state":"submitted"}).to_string();
        let event_store=store.clone();let event_device=device.clone();let event_op=operation.clone();
        if cx.background_executor().spawn(async move{event_store.record_event(&event_device,&event_op,"submitted",&event)}).await.is_err() {let _=reply.try_send(Err("outcome_unknown".into()));return;}
        let turns=storage::MobileTurnStore::for_workspace(database,workspace);
        loop {
            let query=turns.clone();let query_key=key.clone();
            let outcome=cx.background_executor().spawn(async move{query.get(&query_key)}).await;
            if let Ok(Some(outcome))=outcome {
                let state=match outcome.state {MobileTurnState::Completed=>"succeeded",MobileTurnState::Failed=>"failed",MobileTurnState::Interrupted=>"interrupted"};
                let payload=json!({"sender":"Agent","text":outcome.summary.unwrap_or_else(||"Mac 未提供这次回复的正文。".into()),"target":target,"task_ref":target.task_ref,"turn_id":key.turn_id,"state":state});
                let event_store=store.clone();let event_device=device.clone();let event_op=operation.clone();let body=payload.to_string();
                let _=cx.background_executor().spawn(async move{event_store.record_event(&event_device,&event_op,"task_result",&body)}).await;
                let _=reply.try_send(Ok(payload));return;
            }
            let current=session.read_with(cx,|app,_|app.mobile_card_snapshots().into_iter().find(|c|c.node_id==target.node_id)).ok().flatten();
            if current.is_none_or(|c|c.run_id!=target.run_id || c.session_id!=target.session_id) {let _=reply.try_send(Err("outcome_unknown".into()));return;}
            cx.background_executor().timer(Duration::from_secs(1)).await;
        }
    }).detach();
    Ok(receiver)
}

/// Recovery reads frozen provider evidence; it never submits an instruction or changes a target.
pub(super) async fn recover(
    database: &Path,
    areas: &[WorkspaceArea],
    mac: &str,
    workspace: &str,
    cx: &mut gpui::AsyncApp,
) -> Result<Value, String> {
    let database = database.to_path_buf();
    let mac = mac.to_owned();
    let workspace = workspace.to_owned();
    let projects = cx
        .update(|cx| {
            areas
                .iter()
                .filter_map(|bound| {
                    let session =
                        crate::workspace::desktop::mobile::session(&database, &bound.area, cx)?;
                    Some((
                        bound.area.id.clone(),
                        session.read(cx).persistence.project_store()?.clone(),
                    ))
                })
                .collect::<std::collections::HashMap<_, _>>()
        })
        .map_err(|e| e.to_string())?;
    cx.background_executor().spawn(async move {
        let store=storage::WorkspaceChatStore::new(database.clone());
        for event in store.pending_results(&mac,&workspace).map_err(|_|"unavailable")? {
            let Some(target)=event.payload.get("target").and_then(|t|serde_json::from_value::<WorkspaceAgentTarget>(t.clone()).ok()) else {continue;};
            let Some(project)=projects.get(&target.area_id) else {continue;};
            let payload=if let Some(turn)=event.payload["turn_id"].as_str() {
                let key=MobileTurnKey{node_id:target.node_id,profile:event.payload["profile"].as_str().unwrap_or_default().into(),run_id:target.run_id.clone().unwrap_or_default(),session_id:target.session_id.clone().unwrap_or_default(),turn_id:turn.into()};
                let turns=storage::MobileTurnStore::for_workspace(database.clone(),format!("desktop:{}:{}",workspace,target.area_id));
                turns.get(&key).ok().flatten().map(|outcome|json!({"sender":"Agent","text":outcome.summary,"target":target,"task_ref":target.task_ref,"turn_id":turn,"state":match outcome.state{MobileTurnState::Completed=>"succeeded",MobileTurnState::Failed=>"failed",MobileTurnState::Interrupted=>"interrupted"}}))
            } else {
                let Some(reference)=event.payload["task_ref"].as_str() else {continue;};
                project.agent_dispatch_task_store().get_by_task_ref(reference).ok().flatten().and_then(|task|task_payload(&task,&target))
            };
            if let Some(payload)=payload {store.record_event(&event.device_id,&event.operation_id,"task_result",&payload.to_string()).map_err(|_|"unavailable")?;}
        }
        Ok(json!({}))
    }).await
}
