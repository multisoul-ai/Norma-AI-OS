use super::*;
use agent::context::*;
use gpui::TestAppContext;

/// 场景：确定性直呼跳过模型时，已确认的文本约束仍须传给接收 Agent。
/// 构造：生产 App 实体、隔离 PTY 和共享账本，保存“只修改前端”并创建 Alan。
/// 执行：直接经过生产最终执行边界派发“修复登录”，检查实际协调器正文。
/// 预期：原任务、当前约束和完整来源同在正文；只提交一个可追踪任务。
#[test]
fn context_dispatch_handoff_carries_current_constraint_to_actual_task() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let mut test = TestAppContext::single();
    let (entity, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    let service = entity.update(cx, |app, cx| {
        app.enter_canvas_for_test(dir.path(), cx);
        app.shared_context_service().unwrap()
    });
    let saved = crate::app::shared_tokio_runtime()
        .unwrap()
        .block_on(service.run(Some("limit".into()), |access| {
            let input = access.record_input("limit", ContextOrigin::Voice, "只修改前端")?;
            access.update(
                "limit",
                &input.reference,
                &ContextStateUpdate {
                    input: input.reference.clone(),
                    expected_revision: input.reference.revision,
                    change: ContextStateChange::Constraint {
                        key: "frontend".into(),
                        text: "只修改前端".into(),
                        supersedes: None,
                        enforcement: ConstraintEnforcement::Communicated,
                    },
                },
            )
        }))
        .unwrap();
    // 空原文不能被约束“补成任务”；原文小于 32 KiB、合成后超限也必须在建卡前拒绝。
    for task in ["  ".to_owned(), "x".repeat(32 * 1024 - 64)] {
        entity.update_in(cx, |app, window, cx| {
            let before = app.runtime.document.nodes().len();
            let definitions = app.agent_definitions.definitions().len();
            let rejected = execute_norma_adapter_command_with_source(
                app,
                Some(window),
                cx,
                AdapterCommand::CreateAgent {
                    profile: norma::protocol::AgentProfile::Codex,
                    task: Some(task),
                },
                agent::AgentInputSource::Voice,
            );
            assert!(rejected.is_err(), "无效合成正文必须拒绝");
            assert_eq!(
                app.runtime.document.nodes().len(),
                before,
                "拒绝不能创建卡片"
            );
            assert_eq!(
                app.agent_definitions.definitions().len(),
                definitions,
                "拒绝不能创建定义"
            );
        });
    }
    entity.update_in(cx, |app, window, _| {
        let created = crate::app::execute_agent_card_command(
            app,
            window,
            norma::protocol::CardCommand::CreateAgent {
                name: Some("Alan".into()),
                profile: norma::protocol::AgentProfile::Codex,
                activate: true,
            },
        )
        .unwrap();
        let norma::protocol::CardCommandResult::AgentCreated {
            node_id: Some(node),
            ..
        } = created
        else {
            panic!("应创建卡片")
        };
        app.pending_agent_submissions.mark_ready(node);
    });
    let result = entity
        .update_in(cx, |app, window, cx| {
            execute_norma_adapter_command_with_source(
                app,
                Some(window),
                cx,
                AdapterCommand::AssignAgentTask {
                    agent: "Alan".into(),
                    task: "修复登录".into(),
                },
                agent::AgentInputSource::Voice,
            )
        })
        .unwrap();
    assert!(result.task_id.is_some(), "真实派发必须返回可追踪 task_id");
    entity.read_with(cx, |app, _| {
        let tasks = app.agent_task_coordinator.as_ref().unwrap().tasks();
        assert_eq!(tasks.len(), 1, "只应产生一个实际任务");
        assert_eq!(
            result.task_id,
            Some(tasks[0].task_id),
            "回执必须精确对应带约束的实际任务"
        );
        assert!(tasks[0].task.starts_with("修复登录"), "保留原始任务正文");
        assert!(
            tasks[0].task.contains("只修改前端"),
            "确定性直呼不能丢失此前文本约束"
        );
        assert!(
            tasks[0].task.contains(&saved.reference.event_id),
            "交接必须带约束来源 ID"
        );
        assert!(
            tasks[0].task.contains(&service.scope().area_id),
            "交接必须带作用域来源"
        );
        assert_eq!(
            tasks[0].task.matches(&saved.reference.event_id).count(),
            1,
            "同 scope 不能重复附加来源"
        );
    });
    // 持续会话：纠正后只使用最新约束；撤销后回复原文保持逐字不变。
    let corrected = change(
        &service,
        "correct",
        "只修改后端",
        ContextStateChange::Constraint {
            key: "frontend".into(),
            text: "只修改后端".into(),
            supersedes: Some(saved.reference.clone()),
            enforcement: ConstraintEnforcement::Communicated,
        },
    );
    let reply = entity
        .update(cx, |app, cx| {
            execute_norma_adapter_command_with_source(
                app,
                None,
                cx,
                AdapterCommand::ReplyAgent {
                    agent: "Alan".into(),
                    message: "继续检查".into(),
                },
                agent::AgentInputSource::DoubaoVoice,
            )
        })
        .unwrap();
    entity.read_with(cx, |app, _| {
        let queued = app
            .session_reply_queue
            .peek_reply(reply.reply_id.unwrap())
            .unwrap();
        assert!(
            queued.message.contains("只修改后端"),
            "回复也必须携带最新有效约束"
        );
        assert!(
            !queued.message.contains("只修改前端"),
            "新回复不能复用旧约束"
        );
        assert!(
            queued.message.contains(&corrected.event_id),
            "回复携带新约束引用"
        );
    });
    change(
        &service,
        "revoke",
        "取消限制frontend",
        ContextStateChange::RevokeConstraint {
            key: "frontend".into(),
            supersedes: corrected,
        },
    );
    let reply = entity
        .update(cx, |app, cx| {
            execute_norma_adapter_command_with_source(
                app,
                None,
                cx,
                AdapterCommand::ReplyAgent {
                    agent: "Alan".into(),
                    message: "继续检查".into(),
                },
                agent::AgentInputSource::Voice,
            )
        })
        .unwrap();
    entity.read_with(cx, |app, _| {
        let queued = app
            .session_reply_queue
            .peek_reply(reply.reply_id.unwrap())
            .unwrap();
        assert_eq!(
            queued.message, "继续检查",
            "撤销后不能补入旧约束或空上下文块"
        );
    });
    change(
        &service,
        "stop",
        "不要派发任务",
        ContextStateChange::Constraint {
            key: "stop".into(),
            text: "不要派发任务".into(),
            supersedes: None,
            enforcement: ConstraintEnforcement::Enforced {
                operation: ContextOperation::DispatchTask,
                target: None,
            },
        },
    );
    entity.update(cx, |app, cx| {
        let rejected = execute_norma_adapter_command_with_source(
            app,
            None,
            cx,
            AdapterCommand::ReplyAgent {
                agent: "Alan".into(),
                message: "不可发送".into(),
            },
            agent::AgentInputSource::Voice,
        );
        assert!(matches!(rejected, Err(AdapterError::Rejected(ref message)) if message == "context_operation_restricted"), "必须由 enforced gate 明确拒绝");
        let replies = app.session_reply_queue.interrupt_all();
        assert_eq!(replies.len(), 2, "队列只能包含之前两条已接受回复");
        assert!(!replies.iter().any(|reply| reply.message.contains("不可发送")), "被拒绝回复不能入队");
        assert_eq!(
            app.agent_task_coordinator.as_ref().unwrap().tasks().len(),
            1,
            "不能额外派发任务"
        );
    });
    entity.update(cx, |app, _| app.runtime.kill_all());
}

fn change(
    service: &crate::app::shared_context::SharedContext,
    operation: &str,
    text: &str,
    change: ContextStateChange,
) -> ContextRef {
    let operation = operation.to_owned();
    let text = text.to_owned();
    crate::app::shared_tokio_runtime()
        .unwrap()
        .block_on(service.run(Some(operation.clone()), move |access| {
            let input = access.record_input(&operation, ContextOrigin::Voice, &text)?;
            access.update(
                &operation,
                &input.reference,
                &ContextStateUpdate {
                    input: input.reference.clone(),
                    expected_revision: input.reference.revision,
                    change: change.clone(),
                },
            )
        }))
        .unwrap()
        .reference
}
