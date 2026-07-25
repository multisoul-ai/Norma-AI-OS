fn start_adapter_pump(
    requests: async_channel::Receiver<NormaAdapterRequest>,
    runtime_id: u64,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) {
    cx.spawn_in(window, async move |this, cx| {
        while let Ok(request) = requests.recv().await {
            tracing::debug!(
                runtime_id,
                project_id = %request.context.project_id,
                turn_id = %request.context.turn_id,
                tool_call_id = %request.context.tool_call_id,
                "Norma AgentCard Adapter received tool command"
            );
            let command = request.command.clone();
            let result = this
                .update_in(cx, |app, window, cx| {
                    if !is_current_runtime(app, runtime_id) {
                        return Err(AdapterError::Failed(
                            "Norma Adapter 属于已失效项目".to_string(),
                        ));
                    }
                    execute_norma_adapter_command(app, window, cx, command)
                })
                .unwrap_or_else(|_| {
                    Err(AdapterError::Failed(
                        "Norma Adapter 无法访问 App 上下文".to_string(),
                    ))
                });
            let _ = request.result_tx.send(result);
        }
    })
    .detach();
}

pub(crate) fn execute_norma_adapter_command(
    app: &mut MultisoulApp,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
    command: AdapterCommand,
) -> Result<AdapterResult, AdapterError> {
    match command {
        AdapterCommand::FindAgentTasks {
            task_id,
            agent,
            states,
        } => {
            let default_unfinished = task_id.is_none() && agent.is_none() && states.is_empty();
            let tasks = app
                .agent_task_coordinator
                .as_ref()
                .map(agent::AgentTaskCoordinator::tasks)
                .unwrap_or_default()
                .into_iter()
                .filter(|task| task_id.is_none_or(|expected| task.task_id == expected))
                .filter(|task| {
                    agent.as_ref().is_none_or(|expected| {
                        task.agent.eq_ignore_ascii_case(expected.trim())
                    })
                })
                .filter(|task| states.is_empty() || states.iter().any(|state| state == task.state))
                .filter(|task| {
                    !default_unfinished
                        || matches!(task.state, "queued" | "running" | "cancel_requested")
                })
                .rev()
                .take(20)
                .map(|task| {
                    serde_json::json!({
                        "task_id": task.task_id,
                        "agent": task.agent,
                        "task": task.task,
                        "state": task.state,
                    })
                })
                .collect::<Vec<_>>();
            Ok(AdapterResult::succeeded(format!(
                "找到 {} 个 Agent 任务",
                tasks.len()
            ))
            .with_tasks(tasks))
        }
        AdapterCommand::CreateAgent { profile } => {
            let result = commands::execute_agent_card_command(
                app,
                window,
                CardCommand::CreateAgent {
                    name: None,
                    profile,
                    activate: true,
                },
            )
            .map_err(AdapterError::Rejected)?;
            match result {
                CardCommandResult::AgentCreated {
                    name,
                    node_id: Some(node_id),
                    ..
                } => Ok(AdapterResult::succeeded(format!(
                    "已创建并打开 Agent {name}"
                ))
                .with_node_id(node_id)),
                other => Err(AdapterError::Failed(format!(
                    "创建 Agent 返回了意外回执：{other:?}"
                ))),
            }
        }
        AdapterCommand::AssignAgentTask { agent, task } => {
            let definition_id = app
                .agent_definitions
                .definition_by_address(&agent)
                .map(|definition| definition.id)
                .ok_or_else(|| AdapterError::Rejected(format!("未找到 Agent：{agent}")))?;
            let result = commands::execute_agent_card_command(
                app,
                window,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: agent.clone(),
                        task: task.clone(),
                    }],
                    force_untracked: false,
                },
            )
            .map_err(AdapterError::Rejected)?;
            let CardCommandResult::AgentTasksDispatched { receipts } = result else {
                return Err(AdapterError::Failed(
                    "Agent 派发没有返回任务回执".to_string(),
                ));
            };
            let receipt = receipts
                .first()
                .ok_or_else(|| AdapterError::Failed("Agent 派发回执为空".to_string()))?;
            if matches!(
                receipt.status,
                norma::protocol::AgentTaskReceiptStatus::Failed
            ) {
                return Err(AdapterError::Rejected(format!(
                    "{} 未接收任务",
                    receipt.name
                )));
            }
            let task_id = app
                .agent_task_coordinator
                .as_ref()
                .and_then(|coordinator| coordinator.task_id_for(definition_id, &task))
                .ok_or_else(|| {
                    AdapterError::Failed("AgentCard 已接收但无法确定 task_id".to_string())
                })?;
            Ok(AdapterResult::accepted(format!(
                "{} 的 AgentCard 已接收任务",
                receipt.name
            ))
            .with_task_id(task_id)
            .with_node_id(receipt.node_id))
        }
        AdapterCommand::CancelAgentTask { task_id } => {
            let task_to_cancel = app.agent_task_coordinator.as_ref().and_then(|coordinator| {
                coordinator
                    .tasks()
                    .into_iter()
                    .find(|task| task.task_id == task_id)
            });
            let active_node_id = task_to_cancel.as_ref().and_then(|task| {
                app.runtime.document.nodes().iter().find_map(|node| {
                    let canvas::CanvasNodeKind::Agent(spec) = &node.kind else {
                        return None;
                    };
                    (spec.definition_id == Some(task.definition_id)).then_some(node.id)
                })
            });
            if let Some((node_id, task_was_typed)) =
                app.pending_agent_submissions.task_stage(task_id)
            {
                if task_was_typed {
                    if let Err(error) =
                        command_support::clear_agent_task_input(&mut app.runtime, node_id)
                    {
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
                    return Ok(AdapterResult::succeeded(format!(
                        "尚未提交的任务 #{task_id} 已取消"
                    ))
                    .with_task_id(task_id)
                    .with_node_id(removed.receipt.node_id));
                }
                return Err(AdapterError::Rejected(format!(
                    "未找到可取消的任务 #{task_id}"
                )));
            }

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
                let interrupt = runtime::NodeInput::Key(terminal::KeyInput {
                    key: terminal::Key::Char('c'),
                    ctrl: true,
                    alt: false,
                });
                if let Err(error) = app.runtime.send_node_input(node_id, interrupt) {
                    return Ok(AdapterResult::unknown(format!(
                        "任务 #{task_id} 尚未修改，无法确认中断是否送达：{error}"
                    ))
                    .with_task_id(task_id)
                    .with_node_id(node_id));
                }
            }
            let result = app
                .agent_task_coordinator
                .as_mut()
                .map(|coordinator| coordinator.cancel_task(task_id))
                .unwrap_or(agent::AgentTaskCancellation::NotFound);
            if let Some(writer) = app.persistence.agent_dispatch_writer() {
                match result {
                    agent::AgentTaskCancellation::Cancelled => {
                        writer.persist_cancellation(task_id, false)
                    }
                    agent::AgentTaskCancellation::CancelRequested => {
                        writer.persist_cancellation(task_id, true)
                    }
                    agent::AgentTaskCancellation::NotFound => {}
                }
            }
            match result {
                agent::AgentTaskCancellation::Cancelled => Ok(AdapterResult::succeeded(format!(
                    "排队任务 #{task_id} 已取消"
                ))
                .with_task_id(task_id)),
                agent::AgentTaskCancellation::CancelRequested => Ok(AdapterResult::accepted(
                    format!("运行任务 #{task_id} 已请求取消，等待 AgentCard 确认"),
                )
                .with_task_id(task_id)
                .with_node_id(active_node_id.expect("active cancellation was delivered above"))),
                agent::AgentTaskCancellation::NotFound => Err(AdapterError::Rejected(format!(
                    "未找到可取消的任务 #{task_id}"
                ))),
            }
        }
        command @ (AdapterCommand::ReplyAgent { .. } | AdapterCommand::CancelAgentReply { .. }) => {
            reply_adapter::execute_reply_adapter_command(app, command)
        }
        AdapterCommand::CreateTerminal => map_card_result(
            commands::execute_agent_card_command(
                app,
                window,
                CardCommand::Create {
                    kind: CardKind::Terminal { shell: None },
                    placement: Placement::VisibleArea,
                },
            ),
            "已创建 Terminal",
        ),
        AdapterCommand::CreateBrowser { url } => {
            let url = crate::cards::browser::normalize_browser_url(url.as_deref().unwrap_or(""));
            map_card_result(
                commands::execute_agent_card_command(
                    app,
                    window,
                    CardCommand::Create {
                        kind: CardKind::Browser { url },
                        placement: Placement::VisibleArea,
                    },
                ),
                "已创建 Browser",
            )
        }
        AdapterCommand::CreatePreview {
            path,
            explicit_type,
        } => {
            let workspace_root = app
                .runtime
                .workspace_root()
                .ok_or_else(|| AdapterError::Rejected("当前没有打开的项目".to_string()))?;
            let path = crate::preview::resolve_preview_path(
                workspace_root,
                path.to_string_lossy().as_ref(),
            )
            .map_err(|error| AdapterError::Rejected(error.to_string()))?;
            let area = app.visible_canvas_rect(window);
            let zoom = app.runtime.document.viewport.zoom;
            let bounds = area.inset(crate::canvas::master_layout::layout_gap_for_zoom(zoom));
            let node_id = app.spawn_preview_from_path(
                path,
                explicit_type.map(crate::norma::server::canvas_preview_type),
                bounds,
                area,
                zoom,
                cx,
            );
            Ok(AdapterResult::succeeded("已创建 Preview").with_node_id(node_id))
        }
        AdapterCommand::FocusCard { node_id } => map_card_result(
            commands::execute_agent_card_command(app, window, CardCommand::Focus { node_id }),
            "已聚焦卡片",
        ),
        AdapterCommand::SendTerminalInput {
            node_id,
            text,
            submit,
        } => {
            let is_terminal = app
                .runtime
                .document
                .node(node_id)
                .is_some_and(|node| matches!(node.kind, canvas::CanvasNodeKind::Term(_)));
            if !is_terminal {
                return Err(AdapterError::Rejected(format!(
                    "卡片 #{node_id} 不是 Terminal，不能发送 Terminal 输入"
                )));
            }
            map_card_result(
                commands::execute_agent_card_command(
                    app,
                    window,
                    CardCommand::SendText {
                        node_id,
                        text,
                        submit,
                    },
                ),
                "已发送 Terminal 输入",
            )
        }
        AdapterCommand::DeleteCanvasCard { node_id } => {
            delete_card::close_node(app, window, node_id, "已删除卡片")
        }
        AdapterCommand::DeleteCanvasCardAtPosition { position } => {
            delete_card::close_position(app, window, position)
        }
        AdapterCommand::DeleteAgentCard { agent } => {
            delete_card::close_agent(app, window, &agent)
        }
    }
}

fn map_card_result(
    result: Result<CardCommandResult, String>,
    message: &str,
) -> Result<AdapterResult, AdapterError> {
    match result.map_err(AdapterError::Rejected)? {
        CardCommandResult::Created { node_id }
        | CardCommandResult::Deleted { node_id }
        | CardCommandResult::Focused { node_id }
        | CardCommandResult::Sent { node_id } => {
            Ok(AdapterResult::succeeded(message).with_node_id(node_id))
        }
        other => Err(AdapterError::Failed(format!(
            "Canvas 操作返回了意外回执：{other:?}"
        ))),
    }
}

fn start_event_pump(
    events: async_channel::Receiver<AgentEvent>,
    runtime_id: u64,
    cx: &mut Context<MultisoulApp>,
) {
    cx.spawn(async move |this, cx| {
        while let Ok(event) = events.recv().await {
            diagnostics::event_receive(runtime_id, &event);
            let _ = this.update(cx, |app, cx| {
                if !is_current_runtime(app, runtime_id) {
                    return;
                }
                match event {
                    AgentEvent::DialogueTextDelta { text } => {
                        app.dialogue.on_agent_text_delta(&text, cx);
                    }
                    AgentEvent::DialogueStreamFinished => {
                        app.dialogue.finish_agent_text_stream(cx);
                    }
                    _ => {
                        if let Some(report) = terminal_task_report(&event) {
                            app.dialogue
                                .queue_agent_report_for_current_voice_source(report);
                        }
                    }
                }
                cx.notify();
            });
        }
    })
    .detach();
}

fn terminal_task_report(event: &AgentEvent) -> Option<String> {
    match event {
        AgentEvent::TaskSucceeded { summary, .. } => Some(summary.clone()),
        AgentEvent::TaskPartiallyDispatched { summary, .. } => Some(summary.clone()),
        AgentEvent::TaskFailed { reason, .. } => Some(format!("任务失败：{reason}")),
        AgentEvent::TaskCancelled {
            partial_effects: true,
            ..
        } => Some("任务已取消；已发生的操作不会回滚".to_string()),
        AgentEvent::TaskCancelled { .. } => Some("任务已取消".to_string()),
        AgentEvent::TaskInterrupted { .. } => Some("任务已中断".to_string()),
        _ => None,
    }
}

fn is_current_runtime(app: &MultisoulApp, runtime_id: u64) -> bool {
    app.agent_runtime
        .as_ref()
        .is_some_and(|runtime| runtime.id() == runtime_id)
}
