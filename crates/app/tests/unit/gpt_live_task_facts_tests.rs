use super::*;
use storage::{
    AgentDefinitionStore, AgentDispatchTaskState, AgentDispatchTaskStore, AgentDispatchTracking,
    LiveOperationRequest, LiveOperationReservation, NewAgentDefinition,
    NewOrchestrationAgentDispatchTask,
};

/// AC07/AC10/AC11：任务终态由持久任务事实生成，accepted不提前报完成，重连不丢通知。
/// 构造：真实隔离项目DB内boot-a:7任务与AppDB内accepted回执，使用稳定引用关联。
/// 执行：生产collect读取Submitted→无事实；持久完成→有事实；重建读取→同一事件；ACK→清待交。
/// 预期：只真实Done才报告succeeded，包含稳定引用与node=9，重复扫描不重复排队。
#[test]
fn gpt_live_task_facts_follow_persisted_terminal_state() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let project_db = temp.path().join("project.sqlite");
    let definition = AgentDefinitionStore::new(project_db.clone())
        .create(&NewAgentDefinition {
            name: "Simon".into(),
            runtime_profile: canvas::AgentProfile::Codex,
            workflow_description: String::new(),
        })
        .expect("真实Agent定义");
    let tasks = AgentDispatchTaskStore::new(project_db);
    tasks
        .create_orchestration(&NewOrchestrationAgentDispatchTask {
            runtime_instance_id: "boot-a".into(),
            runtime_task_id: 7,
            task_ref: "boot-a:7".into(),
            root_task_ref: "boot-a:7".into(),
            parent_task_ref: None,
            source_definition_id: None,
            source_node_id: None,
            source_provider: None,
            target_definition_id: definition.id,
            target_node_id: 9,
            original_instruction: "运行测试".into(),
            task_text: "运行测试".into(),
            state: AgentDispatchTaskState::Submitted,
            tracking: AgentDispatchTracking::Managed,
        })
        .expect("真实任务持久化");
    let store = storage::LiveOperationStore::new(temp.path().join("app.sqlite"));
    let command = agent::AdapterCommand::AssignAgentTask {
        agent: "Simon".into(),
        task: "运行测试".into(),
    };
    let LiveOperationReservation::Claimed(lease) = store
        .reserve(&LiveOperationRequest {
            project_id: "/project",
            intent_id: "input-1",
            operation_id: "step-1",
            owner_id: "host",
            command: &command,
        })
        .expect("操作预留")
    else {
        panic!("首次认领")
    };
    store
        .record(
            &lease,
            &Ok(agent::AdapterResult::accepted("已接收")
                .with_task_id(7)
                .with_node_id(9)
                .with_tasks(vec![
                    serde_json::json!({"task_ref":"boot-a:7","task_id":7,"node_id":9}),
                ])),
        )
        .expect("真实accepted");
    let mut cursor = 0;
    assert!(
        collect_facts(&store, "/project", &tasks, &mut cursor)
            .expect("非终态")
            .is_empty(),
        "accepted/submitted不能报完成"
    );
    tasks
        .finish_task(7, AgentDispatchTaskState::Done, None)
        .expect("真实完成落账");
    cursor = 0;
    let first = collect_facts(&store, "/project", &tasks, &mut cursor).expect("终态事实");
    assert_eq!(first.len(), 1, "产生一条终态事实");
    assert!(
        first[0].content.contains("boot-a:7")
            && first[0].content.contains("succeeded")
            && first[0].content.contains("node=9"),
        "来自真实持久身份和终态：{}",
        first[0].content
    );
    cursor = 0;
    let reconnected = collect_facts(&store, "/project", &tasks, &mut cursor).expect("重连重查");
    assert_eq!(
        reconnected[0].event_id, first[0].event_id,
        "断线补交使用同一事实身份"
    );
    store
        .acknowledge_task_fact("/project", &first[0].event_id)
        .expect("仅确认服务端接收");
    cursor = 0;
    assert!(
        collect_facts(&store, "/project", &tasks, &mut cursor)
            .expect("已接收")
            .is_empty(),
        "ACK后不再反复提交，但不宣称完整播报"
    );
}

/// AC11：数据库后台工作未返回时，挂断必须停止等待，后台持久化工作仍可独立结束。
/// 构造：真实spawn_blocking工作被通道barrier阻塞；另一路取消通话token。
/// 执行：启动数据库等待边界→取消→等待50ms上限→释放后台工作→确认其自行完成。
/// 预期：通话等待返回None，后台工作没有被取消；不让慢IO拖住下一次连接。
#[tokio::test]
async fn gpt_live_task_database_wait_is_cancellable_without_aborting_background_work() {
    let (release, blocked) = std::sync::mpsc::channel();
    let (done, completed) = async_channel::bounded(1);
    let job = tokio::task::spawn_blocking(move || {
        let _ = blocked.recv();
        let _ = done.try_send(());
    });
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(50),
        wait_for_database(&cancellation, job),
    )
    .await;
    release
        .send(())
        .expect("释放后台工作，失败路径也不遗留阻塞线程");
    assert!(
        matches!(result, Ok(None)),
        "取消必须停止等待未完成数据库任务"
    );
    completed.recv().await.expect("后台工作仍可独立完成");
}

/// AC10/AC12：查询返回后必须重新读取任务播报开关，不能沿用查询启动时的true。
/// 构造：任务事实已加载、开关已从true改false、可观察输出命令通道。
/// 执行：生产发布入口在关闭后调用；随后恢复开关再发布相同事实。
/// 预期：关闭时零命令且保留事实可重交；恢复时只提交一次，未误标完整播报。
#[test]
fn gpt_live_task_publish_rechecks_latest_enablement() {
    let temp = tempfile::tempdir().expect("隔离路径");
    let (commands, received) = async_channel::bounded(8);
    let (_ack, acks) = async_channel::bounded(8);
    let (setting, enabled) = tokio::sync::watch::channel(true);
    let reporter = TaskReporter {
        database: temp.path().join("app.sqlite"),
        project: "/project".into(),
        tasks: AgentDispatchTaskStore::new(temp.path().join("project.sqlite")),
        commands,
        acknowledgements: acks,
        enabled,
        cancellation: CancellationToken::new(),
    };
    let fact = || storage::LiveTaskFact {
        event_id: "live-task-fact-1".into(),
        content: "真实任务已完成".into(),
    };
    let mut in_flight = std::collections::HashSet::new();
    setting.send_replace(false);
    assert!(
        reporter.publish(vec![fact()], &mut in_flight),
        "关闭不应终止整个媒体通话"
    );
    assert!(
        received.is_empty() && in_flight.is_empty(),
        "关闭后不能新排入播报或消耗未提交事实"
    );
    setting.send_replace(true);
    assert!(
        reporter.publish(vec![fact()], &mut in_flight),
        "恢复后可继续投递"
    );
    assert!(
        matches!(
            received.try_recv(),
            Ok(CallCommand::AppendResult {
                delegation_id: None,
                ..
            })
        ),
        "任务终态以非委派事实投递"
    );
}
