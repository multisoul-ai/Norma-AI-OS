//! Norma 想法工具的 App 端适配：记录、列出、派发工作空间想法。
//!
//! 派发协议：先把想法 CAS 为“派发中”并写占位 task_ref，再走既有派发路径，
//! 成功后用回执里的真实 task_ref 落库；失败回退到待办。重启对账见 `IdeasStore::reconcile_dispatching`。
//! 单行 SQLite 读写与既有派发回执路径同在适配器更新内完成，不进入 PTY/渲染/鼠标热路径。

use super::{adapter_task, commands};
use crate::app::{AppScreen, MultisoulApp};
use crate::motion::{DispatchReceiptVisualStatus, DispatchVisualOrigin};
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

/// 想法流星的飞行约 900ms（见 motion 里的 IDEA_FLIGHT_DURATION）；任务在落点撞击前一刻才开始。
const IDEA_DELIVERY_DELAY: std::time::Duration = std::time::Duration::from_millis(860);

/// 一次想法派发的“谁发起、流星从哪出发”。
#[derive(Clone, Copy)]
pub(crate) struct IdeaFlight {
    pub(crate) origin: DispatchVisualOrigin,
    /// 用户在界面里亲手派发：本身就是明确授权，且按“手动”来源入队。
    pub(crate) user_initiated: bool,
}

/// 左抽屉里想法清单的出发点；对话、语音、键盘派发都从这里起飞，不再借用底部指令栏的飞行效果。
pub(super) fn drawer_origin(
    app: &MultisoulApp,
    cx: &Context<MultisoulApp>,
) -> DispatchVisualOrigin {
    let (x, y) = app
        .desktop_host
        .as_ref()
        .and_then(|host| host.upgrade())
        .map_or((140, 240), |host| {
            host.read(cx).desktop_workspace.idea_flight_anchor
        });
    DispatchVisualOrigin::IdeaRow { x, y }
}

pub(crate) fn assign_idea(
    app: &mut MultisoulApp,
    window: Option<&mut Window>,
    cx: &mut Context<MultisoulApp>,
    idea_id: &str,
    target: IdeaTarget,
    input_source: AgentInputSource,
    flight: IdeaFlight,
) -> Result<AdapterResult, AdapterError> {
    let delivery = IdeaDelivery {
        target,
        flight,
        states: &[],
    };
    assign_idea_with_states(app, window, cx, idea_id, input_source, delivery)
}

/// `states`: goal/constraints at the guarded boundary; idea text lives in storage, so it is decorated here.
pub(crate) struct IdeaDelivery<'a> {
    pub target: IdeaTarget,
    pub flight: IdeaFlight,
    pub states: &'a [(&'a str, &'a agent::context::WorkingState)],
}

pub(crate) fn assign_idea_with_states(
    app: &mut MultisoulApp,
    mut window: Option<&mut Window>,
    cx: &mut Context<MultisoulApp>,
    idea_id: &str,
    input_source: AgentInputSource,
    delivery: IdeaDelivery<'_>,
) -> Result<AdapterResult, AdapterError> {
    let IdeaDelivery {
        target,
        flight,
        states,
    } = delivery;
    let IdeaFlight {
        origin,
        user_initiated,
    } = flight;
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
    let roster_state = match &target {
        IdeaTarget::Agent { node_id, .. } => app
            .agent_roster_for_provider()
            .into_iter()
            .find(|entry| entry.active_node_id == Some(*node_id))
            .map(|entry| format!("tracking={:?} busy={}", entry.tracking, entry.busy)),
        IdeaTarget::NewAgent { .. } => Some("new agent".to_owned()),
    };
    tracing::debug!(
        idea_id,
        ?roster_state,
        user_initiated,
        "[idea-dispatch] begin"
    );
    // 派发给智能体的文本 = 正文 + 附带图片的绝对路径。
    let task_text = match agent::context::context_task_handoff(&ideas.dispatch_text(&idea), states)
    {
        Ok(text) => text,
        Err(error) => {
            if let Err(rollback) = ideas.fail_dispatch(&workspace_id, idea_id) {
                tracing::warn!(%rollback, idea_id, "idea handoff rejected and could not be rolled back");
            }
            return Err(AdapterError::Rejected(error.into()));
        }
    };
    // 流星要飞约 0.9 秒才落到卡片：任务正文等流星到了再输入，否则卡片会在流星抵达前就开始运行。
    let hold_until = std::time::Instant::now() + IDEA_DELIVERY_DELAY;
    let held_node = match &target {
        IdeaTarget::Agent { node_id, .. } => {
            with_target_pending(app, cx, &workspace_id, &area_id, |pending| {
                pending.hold_until(*node_id, hold_until)
            });
            Some(*node_id)
        }
        IdeaTarget::NewAgent { .. } => None,
    };
    // 想法有自己的流星（从左抽屉起飞）；压住派发路径里指令栏的飞行，避免叠成两条。
    app.motion.set_dock_flight_suppressed(true);
    let dispatched = match target {
        IdeaTarget::Agent { node_id, .. } => app.norma_assign_workspace_task_with(
            &workspace_id,
            &area_id,
            node_id,
            &task_text,
            user_initiated,
            cx,
        ),
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
                    task_text.clone(),
                    user_initiated,
                    user_initiated || input_source == AgentInputSource::DoubaoVoice,
                )
            }
        }
    };
    app.motion.set_dock_flight_suppressed(false);
    if let (Some(node_id), Err(_)) = (held_node, &dispatched) {
        with_target_pending(app, cx, &workspace_id, &area_id, |pending| {
            pending.release_hold(node_id, hold_until)
        });
    }
    tracing::debug!(
        idea_id,
        outcome = ?dispatched.as_ref().map(|result| (result.status.clone(), result.task_id, result.node_id)),
        "[idea-dispatch] result"
    );
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
            spawn_idea_flight(app, cx, idea_id, &result, origin);
            notify_host(app, cx);
            Ok(result)
        }
        // 任务文本已经提交、只是没拿到任务编号：不回滚，保持派发中，交给后台对账按正文找回，避免重复派发。
        Err(error @ AdapterError::AcceptedWithoutTaskId(_)) => {
            notify_host(app, cx);
            Err(error)
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
    from: Option<(f32, f32)>,
) -> Result<AdapterResult, AdapterError> {
    // 有具体行位置（拖拽、菜单点击）就从那里起飞；键盘等没有位置的，从左抽屉的出发点起飞。
    let origin = from.map_or_else(
        || drawer_origin(app, cx),
        |(x, y)| DispatchVisualOrigin::IdeaRow {
            x: x.round() as i32,
            y: y.round() as i32,
        },
    );
    let flight = IdeaFlight {
        origin,
        user_initiated: true,
    };
    assign_idea(
        app,
        Some(window),
        cx,
        idea_id,
        target,
        AgentInputSource::Other,
        flight,
    )
}

/// 清单缓存属于桌面宿主；项目会话写库后通知宿主刷新。
/// 刷新要读取各项目会话，而当前会话正处于更新中，不能立刻读自己：放到下一轮再执行。
fn notify_host(app: &MultisoulApp, cx: &mut Context<MultisoulApp>) {
    if let Some(host) = app.desktop_host.clone() {
        cx.spawn(async move |_, cx| {
            let _ = host.update(cx, |host, cx| host.refresh_ideas(cx));
        })
        .detach();
    }
}

fn create_agent_with_task(
    app: &mut MultisoulApp,
    mut window: Option<&mut Window>,
    cx: &mut Context<MultisoulApp>,
    profile: norma::protocol::AgentProfile,
    task: String,
    user_initiated: bool,
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
    let mut receipt = adapter_task::dispatch_agent_task(
        app,
        window,
        cx,
        name.clone(),
        task,
        force_untracked,
        user_initiated,
    )?;
    receipt.message = format!("已创建并打开 Agent {name}，AgentCard 已接收想法");
    receipt.node_id = Some(node_id);
    Ok(receipt)
}

/// 在目标项目会话的等待队列上做一件事：目标就是当前会话时直接用 `app`，否则找宿主里对应的项目会话。
/// 暂缓必须落在拥有该卡片的会话上，写到别的会话里既不起作用，还会泄漏等待记录。
fn with_target_pending(
    app: &mut MultisoulApp,
    cx: &mut Context<MultisoulApp>,
    workspace_id: &str,
    area_id: &str,
    change: impl FnOnce(&mut crate::app::agent_runtime::pending::PendingAgentSubmissions),
) {
    if is_source_area(app, cx, workspace_id, area_id) {
        change(&mut app.pending_agent_submissions);
        return;
    }
    let Some(host) = app.desktop_host.as_ref().and_then(|host| host.upgrade()) else {
        return;
    };
    let session = host
        .read(cx)
        .desktop_workspace
        .sessions
        .get(&(workspace_id.to_owned(), area_id.to_owned()))
        .cloned();
    if let Some(session) = session {
        session.update(cx, |session, _| {
            change(&mut session.pending_agent_submissions)
        });
    }
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

/// 派发成功后播放流星：只在目标卡片属于本会话时播放（跨项目的目标由各自会话渲染）。
fn spawn_idea_flight(
    app: &mut MultisoulApp,
    cx: &mut Context<MultisoulApp>,
    idea_id: &str,
    result: &AdapterResult,
    origin: DispatchVisualOrigin,
) {
    let Some(node_id) = result
        .node_id
        .filter(|id| app.runtime.document.node(*id).is_some())
    else {
        return;
    };
    let queued = result.task_id.is_some_and(|task_id| {
        app.agent_task_coordinator
            .as_ref()
            .is_some_and(|coordinator| {
                coordinator
                    .tasks()
                    .iter()
                    .any(|task| task.task_id == task_id && task.state == "queued")
            })
    });
    let status = if queued {
        DispatchReceiptVisualStatus::Queued
    } else {
        DispatchReceiptVisualStatus::Submitted
    };
    app.motion
        .spawn_dispatch_receipt_from(format!("idea:{idea_id}"), origin, node_id, status);
    app.wake_animation_frame_loop();
    cx.notify();
}

#[cfg(test)]
#[path = "../../../tests/unit/desktop_idea_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/idea_context_handoff_tests.rs"]
mod handoff_tests;
