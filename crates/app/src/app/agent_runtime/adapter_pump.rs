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
            let input_source = request.context.input_source;
            let result = this
                .update_in(cx, |app, window, cx| {
                    if !is_current_runtime(app, runtime_id) {
                        return Err(AdapterError::Failed(
                            "Norma Adapter 属于已失效项目".to_string(),
                        ));
                    }
                    execute_norma_adapter_command_with_source(
                        app,
                        Some(window),
                        cx,
                        command,
                        input_source,
                    )
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

#[cfg(any(test, feature = "e2e-test-support"))]
pub(crate) fn execute_norma_adapter_command(
    app: &mut MultisoulApp,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
    command: AdapterCommand,
) -> Result<AdapterResult, AdapterError> {
    execute_norma_adapter_command_with_source(
        app,
        Some(window),
        cx,
        command,
        agent::AgentInputSource::Other,
    )
}

pub(crate) fn execute_norma_adapter_command_with_source(
    app: &mut MultisoulApp,
    mut window: Option<&mut Window>,
    cx: &mut Context<MultisoulApp>,
    command: AdapterCommand,
    input_source: agent::AgentInputSource,
) -> Result<AdapterResult, AdapterError> {
    match command {
        AdapterCommand::Memory(_) | AdapterCommand::WorkspaceMemory { .. } => Err(AdapterError::Rejected("记忆 IO 必须经过后台服务".into())),
        AdapterCommand::CaptureIdea { body } => {
            idea_commands::capture_idea(app, cx, &body, input_source)
        }
        AdapterCommand::ListIdeas => idea_commands::list_ideas(app, cx),
        AdapterCommand::AssignIdea { idea_id, target } => {
            idea_commands::assign_idea(app, window, cx, &idea_id, target, input_source)
        }
        AdapterCommand::WorkspaceSnapshot { scope, area_id } => {
            app.norma_workspace_snapshot(scope, area_id.as_deref(), cx)
        }
        AdapterCommand::AssignWorkspaceTask {
            workspace_id,
            area_id,
            node_id,
            task,
        } => app.norma_assign_workspace_task(&workspace_id, &area_id, node_id, &task, cx),
        AdapterCommand::OpenWorkspaceArtifact {
            workspace_id,
            area_id,
            node_id,
            turn_id,
            source_path,
            version,
        } => app.norma_open_workspace_artifact(
            &workspace_id,
            &area_id,
            node_id,
            &turn_id,
            &source_path,
            &version,
            cx,
        ),
        AdapterCommand::LocateWorkspaceAgent {
            workspace_id,
            area_id,
            node_id,
            approval_run_id,
            approval_turn_id,
        } => app.norma_locate_workspace_agent(
            &workspace_id,
            &area_id,
            node_id,
            approval_run_id.as_deref(),
            approval_turn_id.as_deref(),
            window.as_deref_mut().ok_or_else(|| {
                AdapterError::Rejected("定位 Agent 需要桌面窗口".into())
            })?,
            cx,
        ),
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
                    agent
                        .as_ref()
                        .is_none_or(|expected| task.agent.eq_ignore_ascii_case(expected.trim()))
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
            Ok(
                AdapterResult::succeeded(format!("找到 {} 个 Agent 任务", tasks.len()))
                    .with_tasks(tasks),
            )
        }
        AdapterCommand::CreateAgent { profile, task } => {
            let result = commands::execute_agent_card_command_with_optional_window(
                app,
                window.as_deref_mut(),
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
                } => match task {
                    Some(task) => {
                        let mut receipt = adapter_task::dispatch_agent_task(
                            app,
                            window.as_deref_mut(),
                            cx,
                            name.clone(),
                            task,
                            input_source == agent::AgentInputSource::DoubaoVoice,
                        )?;
                        receipt.message =
                            format!("已创建并打开 Agent {name}，AgentCard 已接收任务");
                        receipt.node_id = Some(node_id);
                        Ok(receipt)
                    }
                    None => Ok(
                        AdapterResult::succeeded(format!("已创建并打开 Agent {name}"))
                            .with_node_id(node_id),
                    ),
                },
                other => Err(AdapterError::Failed(format!(
                    "创建 Agent 返回了意外回执：{other:?}"
                ))),
            }
        }
        AdapterCommand::AssignAgentTask { agent, task } => adapter_task::dispatch_agent_task(
            app,
            window.as_deref_mut(),
            cx,
            agent,
            task,
            input_source == agent::AgentInputSource::DoubaoVoice,
        ),
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
                    shift: false,
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
                    agent::AgentTaskCancellation::NotFound
                    | agent::AgentTaskCancellation::AlreadySubmitted => {}
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
                agent::AgentTaskCancellation::CancelRequested => Ok(AdapterResult::accepted(
                    format!("运行任务 #{task_id} 已请求取消，等待 AgentCard 确认"),
                )
                .with_task_id(task_id)
                .with_node_id(active_node_id.expect("active cancellation was delivered above"))),
                agent::AgentTaskCancellation::NotFound => Err(AdapterError::Rejected(format!(
                    "未找到可取消的任务 #{task_id}"
                ))),
                agent::AgentTaskCancellation::AlreadySubmitted => Err(AdapterError::Rejected(
                    "消息已交给 Agent CLI，请在卡片内管理其消息队列".to_string(),
                )),
            }
        }
        command @ AdapterCommand::ReplyAgent { .. } => {
            let result = reply_adapter::execute_reply_adapter_command(app, command)?;
            if let Some(window) = window.as_deref_mut().filter(|_| {
                matches!(
                    input_source,
                    agent::AgentInputSource::Voice | agent::AgentInputSource::DoubaoVoice
                )
            }) {
                let visible_area = app.visible_canvas_rect(window);
                if dispatch_effects::spawn_accepted_session_reply(
                    app,
                    result.reply_id,
                    result.node_id,
                    result.reply_status.as_deref(),
                    visible_area,
                ) {
                    cx.notify();
                }
            }
            Ok(result)
        }
        command @ AdapterCommand::CancelAgentReply { .. } => {
            reply_adapter::execute_reply_adapter_command(app, command)
        }
        AdapterCommand::CreateTerminal => map_card_result(
            commands::execute_agent_card_command_with_optional_window(
                app,
                window.as_deref_mut(),
                CardCommand::Create {
                    kind: CardKind::Terminal { shell: None },
                    placement: Placement::VisibleArea,
                },
            ),
            "已创建 Terminal",
        ),
        AdapterCommand::CreateBrowser { url } => {
            let url = crate::cards::browser::normalize_browser_url(url.as_deref().unwrap_or(""));
            if desktop_content_tool_available(app) {
                return open_desktop_browser_tab_from_tool(app, url, cx);
            }
            map_card_result(
                commands::execute_agent_card_command_with_optional_window(
                    app,
                    window.as_deref_mut(),
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
            if desktop_content_tool_available(app) {
                return open_desktop_preview_tab_from_tool(app, path, cx);
            }
            let workspace_root = app
                .runtime
                .workspace_root()
                .ok_or_else(|| AdapterError::Rejected("当前没有打开的项目".to_string()))?;
            let path = crate::preview::resolve_preview_path(
                workspace_root,
                path.to_string_lossy().as_ref(),
            )
            .map_err(|error| AdapterError::Rejected(error.to_string()))?;
            let area = app.mobile_canvas_area(window.as_deref_mut());
            let zoom = app.runtime.document.viewport.zoom;
            let bounds = app.next_tiled_node_bounds_for_area(area, zoom);
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
            commands::execute_agent_card_command_with_optional_window(
                app,
                window.as_deref_mut(),
                CardCommand::Focus { node_id },
            ),
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
                commands::execute_agent_card_command_with_optional_window(
                    app,
                    window.as_deref_mut(),
                    CardCommand::SendText {
                        node_id,
                        text,
                        submit,
                    },
                ),
                "已发送 Terminal 输入",
            )
        }
        AdapterCommand::DeleteCanvasCard { node_id } => delete_card::close_node(
            app,
            window.as_deref_mut(),
            node_id,
            "已删除卡片",
            input_source != agent::AgentInputSource::DoubaoVoice,
        ),
        AdapterCommand::DeleteCanvasCardAtPosition { position } => delete_card::close_position(
            app,
            window.as_deref_mut(),
            position,
            input_source != agent::AgentInputSource::DoubaoVoice,
        ),
        AdapterCommand::DeleteAgentCard { agent } => delete_card::close_agent(
            app,
            window,
            &agent,
            input_source != agent::AgentInputSource::DoubaoVoice,
        ),
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
