use storage::{
    ApprovalAuditStore, BackgroundApprovalAudit, MobileTaskBaselineStore, MobileTaskDeliveryStore,
    MobileTurnKey, MobileTurnOutcome, MobileTurnState, MobileTurnStore, NewMobileCommand,
    TaskBaseline, TaskDeliveryContent,
};
use workspace::review::{ContentKind, FileChange, FileVersion, ProjectReview};

pub fn seed(database: &std::path::Path, source: &NewMobileCommand, turn: &str) {
    seed_with_summary(database, source, turn, "冻结成果");
}

pub fn seed_with_summary(database: &std::path::Path, source: &NewMobileCommand, turn: &str, summary: &str) {
    let key = MobileTurnKey {
        node_id: 2,
        profile: "Codex".into(),
        run_id: "review-run".into(),
        session_id: "review-session".into(),
        turn_id: turn.into(),
    };
    let review = ProjectReview {
        base_commit: None,
        conflicted: false,
        files: vec![FileChange {
            path: "result.txt".into(),
            status: " M".into(),
            before: Some(FileVersion {
                kind: ContentKind::Text,
                executable: false,
                bytes: b"before\n".to_vec(),
            }),
            after: Some(FileVersion {
                kind: ContentKind::Text,
                executable: false,
                bytes: "结果\n".repeat(20_000).into_bytes(),
            }),
            index: None,
            diff: Some("-before\n+结果\n".into()),
            staged_diff: None,
        }],
    };
    MobileTaskBaselineStore::new(database.into())
        .record(
            source,
            &key,
            &TaskBaseline::Available {
                review: review.clone(),
            },
        )
        .expect("原任务成果必须绑定实际派工记录");
    let writer = BackgroundApprovalAudit::new(
        ApprovalAuditStore::new(database.with_extension("audit.sqlite")).with_turn_store(
            MobileTurnStore::for_workspace(database.into(), source.workspace_id.clone()),
        ),
    )
    .expect("隔离成果写入器");
    writer
        .record_turn(MobileTurnOutcome {
            key: key.clone(),
            state: MobileTurnState::Completed,
            summary: Some(summary.into()),
        })
        .expect("终态入队")
        // Existing writer budget: local SQLite commit 1s + CI scheduling margin 4s = 5s.
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("等待写入")
        .expect("终态保存");
    MobileTaskDeliveryStore::new(database.into())
        .record(
            &source.workspace_id,
            &key,
            &TaskDeliveryContent::Available { review },
        )
        .expect("冻结成果正文");
}

/// Explicit test-only foreground substitute for native UI tests; never counts as Coding Agent execution.
pub fn serve_ui_commands(
    commands: async_channel::Receiver<app::LiveCommandRequest>,
    directory: std::path::PathBuf,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut task_id = 100;
        while !stop.load(std::sync::atomic::Ordering::Acquire) {
            let Ok(call) = commands.try_recv() else {
                // UI commands are sparse; 10ms polling bounds test response latency without busy spinning.
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            };
            assert!(call.lease.begin(), "UI fixture只消费一次合法前台执行权");
            let database = directory.join("app.sqlite");
            let result = match &call.request.command {
                app::MobileCommand::AcceptDelivery { key, fingerprint } => {
                    storage::MobileCommandStore::new(database).accept_delivery(&call.source, key, fingerprint)
                        .expect("原验收事务保存fixture成果决定").result()
                }
                app::MobileCommand::ThreadTask { agent_definition_id, task, context } => {
                    let project = storage::ProjectStore::for_database_paths(
                        &storage::DatabasePaths::isolated_test(&directory), &directory).expect("UI临时项目库");
                    let text = format!("工作线程上下文（参考信息）：\n{context}\n\n当前任务：\n{task}");
                    let task_ref = format!("ui-fixture:{task_id}");
                    project.agent_dispatch_task_store().create_orchestration(&storage::NewOrchestrationAgentDispatchTask {
                        runtime_instance_id: "ui-fixture".into(), runtime_task_id: task_id,
                        task_ref: task_ref.clone(), root_task_ref: task_ref.clone(), parent_task_ref: None,
                        source_definition_id: None, source_node_id: None, source_provider: None,
                        target_definition_id: *agent_definition_id, target_node_id: 2,
                        original_instruction: text.clone(), task_text: text, state: storage::AgentDispatchTaskState::Submitted,
                        tracking: storage::AgentDispatchTracking::Managed,
                    }).expect("UI修改任务fixture入库");
                    project.agent_dispatch_task_store().finish_task_with_result(task_id, storage::AgentDispatchTaskState::Done, None,
                        Some(&storage::StoredAgentDispatchResult { text: Some("修改已完成，请核对文件（测试回执）。".into()), truncated: false }))
                        .expect("UI修改任务fixture回复");
                    seed(&database, &call.source, &format!("ui-turn-{task_id}"));
                    let result = serde_json::json!({"status":"accepted","task_id":task_id,"task_ref":task_ref,"node_id":2});
                    task_id += 1;
                    result
                }
                _ => panic!("UI fixture不能执行验收/修改范围外的命令"),
            };
            call.reply.send(Ok(result)).expect("UI必须收到原入口回执");
        }
    })
}
