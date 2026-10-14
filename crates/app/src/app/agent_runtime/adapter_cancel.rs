use super::command_support;
use crate::app::MultisoulApp;
use agent::{AdapterError, AdapterResult};

pub(super) fn cancel(
    app: &mut MultisoulApp,
    task_id: u64,
    expected_task_ref: Option<&str>,
    connector: bool,
    cx: &mut gpui::Context<MultisoulApp>,
) -> Result<AdapterResult, AdapterError> {
    let task_to_cancel = app.agent_task_coordinator.as_ref().and_then(|coordinator| {
        coordinator
            .tasks()
            .into_iter()
            .find(|task| task.task_id == task_id)
    });
    if expected_task_ref.is_some_and(|expected| {
        task_to_cancel
            .as_ref()
            .is_none_or(|task| task.task_ref != expected)
    }) {
        return Err(AdapterError::Rejected("task_not_found".into()));
    }
    if let Some(task) = task_to_cancel
        .as_ref()
        .filter(|task| !matches!(task.state, "queued" | "running" | "cancel_requested"))
    {
        return Ok(AdapterResult::succeeded("任务已结束")
            .with_task_id(task_id)
            .with_reply_status(task.state));
    }
    let active_node_id = task_to_cancel.as_ref().map(|task| task.target_node_id);
    if let Some((node_id, task_was_typed)) = app.pending_agent_submissions.task_stage(task_id) {
        if task_was_typed
            && task_to_cancel
                .as_ref()
                .is_none_or(|task| !matches_target(app, task, node_id))
        {
            return Ok(AdapterResult::unknown("目标卡片已改变，原操作待核对").with_task_id(task_id));
        }
        if task_was_typed {
            if let Err(error) = command_support::clear_agent_task_input(&mut app.runtime, node_id) {
                return Ok(AdapterResult::unknown(format!(
                    "任务 #{task_id} 尚未提交，但无法确认输入区是否清空：{error}"
                ))
                .with_task_id(task_id)
                .with_node_id(node_id));
            }
        }
        let removed = app
            .pending_agent_submissions
            .remove_task(task_id)
            .expect("pending task stage was found immediately above");
        let (result, next) = app
            .agent_task_coordinator
            .as_mut()
            .map(|coordinator| coordinator.cancel_unsubmitted_task(task_id))
            .unwrap_or((agent::AgentTaskCancellation::NotFound, None));
        if let Some(next) = next {
            app.pending_agent_submissions.enqueue_immediate(next);
        }
        if result == agent::AgentTaskCancellation::Cancelled {
            if let Some(writer) = app.persistence.agent_dispatch_writer() {
                writer.persist_cancellation(task_id, false);
            }
            return Ok(
                AdapterResult::succeeded(format!("尚未提交的任务 #{task_id} 已取消"))
                    .with_task_id(task_id)
                    .with_node_id(removed.receipt.node_id),
            );
        }
        return Err(AdapterError::Rejected(format!(
            "未找到可取消的任务 #{task_id}"
        )));
    }

    let mut confirmation = None;
    let is_active = task_to_cancel
        .as_ref()
        .is_some_and(|task| matches!(task.state, "running" | "cancel_requested"));
    if is_active {
        let Some(node_id) = active_node_id else {
            return Ok(AdapterResult::unknown(format!(
                "任务 #{task_id} 尚未修改，但找不到对应 AgentCard"
            ))
            .with_task_id(task_id));
        };
        if task_to_cancel
            .as_ref()
            .is_none_or(|task| !matches_target(app, task, node_id))
        {
            return Ok(AdapterResult::unknown("目标卡片已改变，原操作待核对").with_task_id(task_id));
        }
        if connector
            && task_to_cancel
                .as_ref()
                .is_some_and(|task| task.state == "cancel_requested")
        {
            return Ok(AdapterResult::accepted("运行任务已请求取消，等待确认")
                .with_task_id(task_id)
                .with_node_id(node_id));
        }
        let card = app
            .runtime
            .mobile_card_snapshots()
            .into_iter()
            .find(|card| card.node_id == node_id);
        if let Some(card) = card.clone().filter(|card| {
            connector
                && matches!(card.profile.as_str(), "Codex" | "Claude")
                && card.run_id.is_some()
                && card.turn_id.is_some()
        }) {
            if app.agent_hook_server.has_pending() {
                return Err(AdapterError::Rejected("agent_rejected".into()));
            }
            let run = card.run_id.as_deref().expect("run");
            let turn = card.turn_id.as_deref().expect("turn");
            let waiting = if card.profile == "Codex"
                && app
                    .runtime
                    .mobile_native_session(node_id)
                    .is_ok_and(|native| native.is_none())
            {
                app.runtime.interrupt_mobile_codex(node_id, run, turn)
            } else {
                app.interrupt_mobile_terminal(node_id, run, turn, cx)
            };
            match waiting {
                Ok(waiting) => confirmation = Some((card, Some(waiting))),
                Err(error) => {
                    return Ok(AdapterResult::unknown(format!("中断无法确认：{error}"))
                        .with_task_id(task_id)
                        .with_node_id(node_id))
                }
            }
        } else {
            let interrupt = runtime::NodeInput::Key(terminal::KeyInput {
                key: terminal::Key::Char('c'),
                ctrl: true,
                alt: false,
                shift: false,
            });
            if let Err(error) = app.runtime.send_node_input(node_id, interrupt) {
                return Ok(AdapterResult::unknown(format!("中断无法送达：{error}"))
                    .with_task_id(task_id)
                    .with_node_id(node_id));
            }
            // Old/untracked turns may have no Provider identity; still bound the cancellation wait.
            if connector {
                confirmation = card.map(|card| (card, None));
            }
        }
    }
    let result = app
        .agent_task_coordinator
        .as_mut()
        .map(|coordinator| coordinator.cancel_task(task_id))
        .unwrap_or(agent::AgentTaskCancellation::NotFound);
    if connector && result == agent::AgentTaskCancellation::CancelRequested {
        app.pending_agent_submissions
            .mark_connector_cancellation(task_id, active_node_id.expect("active target"));
    }
    if let Some(writer) = app.persistence.agent_dispatch_writer() {
        match result {
            agent::AgentTaskCancellation::Cancelled => writer.persist_cancellation(task_id, false),
            agent::AgentTaskCancellation::CancelRequested => {
                writer.persist_cancellation(task_id, true)
            }
            agent::AgentTaskCancellation::NotFound
            | agent::AgentTaskCancellation::AlreadySubmitted => {}
        }
    }
    if let Some((card, waiting)) = confirmation {
        if result == agent::AgentTaskCancellation::CancelRequested {
            super::adapter_cancel_confirmation::watch(
                task_to_cancel.as_ref().expect("target").clone(),
                card,
                waiting,
                cx,
            );
        }
    }
    match result {
        agent::AgentTaskCancellation::Cancelled => {
            app.pending_agent_submissions.remove_task(task_id);
            Ok(
                AdapterResult::succeeded(format!("排队任务 #{task_id} 已取消"))
                    .with_task_id(task_id),
            )
        }
        agent::AgentTaskCancellation::CancelRequested => Ok(AdapterResult::accepted(format!(
            "运行任务 #{task_id} 已请求取消，等待 AgentCard 确认"
        ))
        .with_task_id(task_id)
        .with_node_id(active_node_id.expect("active cancellation was delivered above"))),
        agent::AgentTaskCancellation::NotFound => Err(AdapterError::Rejected(format!(
            "未找到可取消的任务 #{task_id}"
        ))),
        agent::AgentTaskCancellation::AlreadySubmitted => Err(AdapterError::AlreadySubmitted),
    }
}

fn matches_target(app: &MultisoulApp, task: &agent::AgentCoordinatedTask, node_id: u64) -> bool {
    app.runtime.document.node(node_id).is_some_and(|node|matches!(&node.kind,canvas::CanvasNodeKind::Agent(spec) if spec.definition_id == Some(task.definition_id)))
}
