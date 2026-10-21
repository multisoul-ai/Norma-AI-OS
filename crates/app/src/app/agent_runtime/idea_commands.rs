//! Norma 想法工具的 App 端适配：记录、列出、派发工作空间想法。
//!
//! 派发协议：先把想法 CAS 为“派发中”并写占位 task_ref，再走既有派发路径，
//! 成功后用回执里的真实 task_ref 落库；失败回退到待办。重启对账见 `IdeasStore::reconcile_dispatching`。
//! 单行 SQLite 读写与既有派发回执路径同在适配器更新内完成，不进入 PTY/渲染/鼠标热路径。

use super::{adapter_task, commands};
use crate::app::{AppScreen, MultisoulApp};
use agent::{AdapterError, AdapterResult, AgentInputSource, IdeaTarget};
use gpui::{Context, Window};
use norma::protocol::{CardCommand, CardCommandResult};
use serde_json::json;
use storage::{IdeaSource, IdeasStore, StorageError, StoredIdea};

pub(super) fn capture_idea(
    app: &mut MultisoulApp,
    cx: &mut Context<MultisoulApp>,
    body: &str,
    input_source: AgentInputSource,
) -> Result<AdapterResult, AdapterError> {
    let workspace_id = active_workspace_id(app, cx)?;
    let source = match input_source {
        AgentInputSource::Voice
        | AgentInputSource::DoubaoVoice
        | AgentInputSource::GptLiveVoice => IdeaSource::Voice,
        _ => IdeaSource::Chat,
    };
    let idea = store(app)
        .capture(&workspace_id, body, source)
        .map_err(storage_error)?;
    notify_host(app, cx);
    Ok(
        AdapterResult::succeeded(format!("已记下想法：{}", idea.body))
            .with_tasks(vec![idea_json(&idea)]),
    )
}

pub(super) fn list_ideas(
    app: &MultisoulApp,
    cx: &Context<MultisoulApp>,
) -> Result<AdapterResult, AdapterError> {
    let workspace_id = active_workspace_id(app, cx)?;
    let ideas = store(app).list(&workspace_id).map_err(storage_error)?;
    let todo = ideas
        .iter()
        .filter(|idea| idea.state == workspace::ideas::IdeaState::Todo)
        .count();
    Ok(
        AdapterResult::succeeded(format!("共 {} 条想法，其中待办 {todo} 条", ideas.len()))
            .with_tasks(ideas.iter().map(idea_json).collect()),
    )
}

pub(crate) fn assign_idea(
    app: &mut MultisoulApp,
    mut window: Option<&mut Window>,
    cx: &mut Context<MultisoulApp>,
    idea_id: &str,
    target: IdeaTarget,
    input_source: AgentInputSource,
) -> Result<AdapterResult, AdapterError> {
    let workspace_id = active_workspace_id(app, cx)?;
    let (target_workspace, area_id) = match &target {
        IdeaTarget::Agent {
            workspace_id,
            area_id,
            ..
        }
        | IdeaTarget::NewAgent {
            workspace_id,
            area_id,
            ..
        } => (workspace_id.as_str(), area_id.as_str()),
    };
    if target_workspace != workspace_id {
        return Err(AdapterError::Rejected(
            "当前工作空间已变更，请重新查询目标".into(),
        ));
    }
    let area_id = area_id.to_owned();
    let ideas = store(app);
    let idea = ideas
        .begin_dispatch(&workspace_id, idea_id, &format!("idea:{idea_id}"))
        .map_err(storage_error)?
        .ok_or_else(|| AdapterError::Rejected("这条想法不在待办中，可能已指派或已删除".into()))?;
    let dispatched = match target {
        IdeaTarget::Agent { node_id, .. } => {
            app.norma_assign_workspace_task(&workspace_id, &area_id, node_id, &idea.body, cx)
        }
        IdeaTarget::NewAgent { profile, .. } => {
            if !is_source_area(app, cx, &workspace_id, &area_id) {
                Err(AdapterError::Rejected(
                    "新建智能体目前只支持当前展开的项目区域".into(),
                ))
            } else {
                create_agent_with_task(
                    app,
                    window.as_deref_mut(),
                    cx,
                    profile,
                    idea.body.clone(),
                    input_source == AgentInputSource::DoubaoVoice,
                )
            }
        }
    };
    match dispatched {
        Ok(result) => {
            let node_id = result.node_id.unwrap_or_default();
            let task_ref = result
                .task_id
                .and_then(|task_id| task_ref_for(app, cx, &workspace_id, &area_id, task_id));
            if let Err(error) = ideas.mark_assigned(
                &workspace_id,
                idea_id,
                &area_id,
                node_id,
                task_ref.as_deref(),
            ) {
                tracing::warn!(%error, idea_id, "idea dispatched but assignment could not be recorded");
            }
            notify_host(app, cx);
            Ok(result)
        }
        Err(error) => {
            if let Err(rollback) = ideas.fail_dispatch(&workspace_id, idea_id) {
                tracing::warn!(%rollback, idea_id, "idea dispatch failed and could not be rolled back");
            }
            notify_host(app, cx);
            Err(error)
        }
    }
}

/// 界面入口：来源按普通输入处理。
pub(crate) fn assign_idea_for_ui(
    app: &mut MultisoulApp,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
    idea_id: &str,
    target: IdeaTarget,
) -> Result<AdapterResult, AdapterError> {
    assign_idea(
        app,
        Some(window),
        cx,
        idea_id,
        target,
        AgentInputSource::Other,
    )
}

/// 清单缓存属于桌面宿主；项目会话写库后通知宿主后台刷新。
fn notify_host(app: &MultisoulApp, cx: &mut Context<MultisoulApp>) {
    if let Some(host) = app.desktop_host.as_ref().and_then(|host| host.upgrade()) {
        host.update(cx, |host, cx| host.refresh_ideas(cx));
    }
}

fn create_agent_with_task(
    app: &mut MultisoulApp,
    mut window: Option<&mut Window>,
    cx: &mut Context<MultisoulApp>,
    profile: norma::protocol::AgentProfile,
    task: String,
    force_untracked: bool,
) -> Result<AdapterResult, AdapterError> {
    let created = commands::execute_agent_card_command_with_optional_window(
        app,
        window.as_deref_mut(),
        CardCommand::CreateAgent {
            name: None,
            profile,
            activate: true,
        },
    )
    .map_err(AdapterError::Rejected)?;
    let CardCommandResult::AgentCreated {
        name,
        node_id: Some(node_id),
        ..
    } = created
    else {
        return Err(AdapterError::Failed("创建 Agent 返回了意外回执".into()));
    };
    let mut receipt =
        adapter_task::dispatch_agent_task(app, window, cx, name.clone(), task, force_untracked)?;
    receipt.message = format!("已创建并打开 Agent {name}，AgentCard 已接收想法");
    receipt.node_id = Some(node_id);
    Ok(receipt)
}

/// 派发回执只带运行时内的 task_id；真实 task_ref 在目标项目的协调器里。
fn task_ref_for(
    app: &MultisoulApp,
    cx: &Context<MultisoulApp>,
    workspace_id: &str,
    area_id: &str,
    task_id: u64,
) -> Option<String> {
    let find = |coordinator: &agent::AgentTaskCoordinator| {
        coordinator
            .tasks()
            .into_iter()
            .find(|task| task.task_id == task_id)
            .map(|task| task.task_ref)
    };
    if is_source_area(app, cx, workspace_id, area_id) {
        return app.agent_task_coordinator.as_ref().and_then(find);
    }
    let host = app.desktop_host.as_ref()?.upgrade()?;
    let session = host
        .read(cx)
        .desktop_workspace
        .sessions
        .get(&(workspace_id.to_owned(), area_id.to_owned()))?
        .clone();
    session
        .read(cx)
        .agent_task_coordinator
        .as_ref()
        .and_then(find)
}

fn is_source_area(
    app: &MultisoulApp,
    cx: &Context<MultisoulApp>,
    workspace_id: &str,
    area_id: &str,
) -> bool {
    let key = (workspace_id.to_owned(), area_id.to_owned());
    match app.desktop_host.as_ref().and_then(|host| host.upgrade()) {
        Some(host) => host
            .read(cx)
            .desktop_workspace
            .sessions
            .get(&key)
            .is_some_and(|session| session.entity_id() == cx.entity().entity_id()),
        None => false,
    }
}

fn active_workspace_id(
    app: &MultisoulApp,
    cx: &Context<MultisoulApp>,
) -> Result<String, AdapterError> {
    let (screen, active) = match app.desktop_host.as_ref().and_then(|host| host.upgrade()) {
        Some(host) => {
            let host = host.read(cx);
            (host.screen, host.desktop_workspace.active_id.clone())
        }
        None => (app.screen, app.desktop_workspace.active_id.clone()),
    };
    if screen != AppScreen::DesktopWorkspace {
        return Err(AdapterError::Rejected("请先打开桌面工作空间".into()));
    }
    active.ok_or_else(|| AdapterError::Rejected("当前没有打开的桌面工作空间".into()))
}

fn store(app: &MultisoulApp) -> IdeasStore {
    IdeasStore::new(app.database_paths.app_db_path().to_owned())
}

fn idea_json(idea: &StoredIdea) -> serde_json::Value {
    json!({
        "idea_id": idea.id,
        "body": idea.body,
        "state": idea.state.as_str(),
        "area_id": idea.assignee_area_id,
        "node_id": idea.assignee_node_id,
    })
}

fn storage_error(error: StorageError) -> AdapterError {
    match error {
        StorageError::InvalidIdea(_) => {
            AdapterError::Rejected("想法正文不能为空，且不超过 500 字".into())
        }
        other => AdapterError::Failed(format!("想法存储失败：{other}")),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/desktop_idea_tests.rs"]
mod tests;
