use super::super::{
    AgentDefinitionStore, AgentDispatchTaskState, AgentDispatchTaskStore, AgentDispatchTracking,
    BackgroundAgentDispatchStore, NewAgentDefinition, NewOrchestrationAgentDispatchTask,
};
use canvas::AgentProfile;

/// 子委派审计：来源、父/根引用、目标与运行实例身份应完整持久化并可按 task_ref 查询。
///
/// 数据构造（含关键数值的推导过程）：
///   definitions = 2：Alan(id=1) + Bella(id=2)
///   runtime_instance_id = "runtime-a"
///   runtime_task_id = 7；task_ref = "runtime-a:7"
///   parent/root = "runtime-a:1"
///   source_node_id=101；target_node_id=102
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Alan 与 Bella 定义。
///   2. 通过 orchestration create seam 写入 Alan → Bella 子任务。
///   3. 按 task_ref 查询并逐字段核对来源、父/根、目标和状态。
///
/// 预期结果：
///   - 正断言：所有关系字段与输入一致，状态为 Queued、tracking=Managed。
///   - 负断言：source_definition_id 不得被目标 Bella id 覆盖，task_ref 不得为空。
#[test]
fn orchestration_task_persists_runtime_source_parent_root_and_target_fields() {
    let temp = tempfile::tempdir().expect("子委派审计测试应创建临时目录");
    let path = temp.path().join("project.sqlite");
    let definitions = AgentDefinitionStore::new(path.clone());
    let alan = definitions
        .create(&NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: String::new(),
        })
        .expect("测试应创建来源 Alan 定义");
    let bella = definitions
        .create(&NewAgentDefinition {
            name: "Bella".to_string(),
            runtime_profile: AgentProfile::Claude,
            workflow_description: String::new(),
        })
        .expect("测试应创建目标 Bella 定义");
    let store = AgentDispatchTaskStore::new(path);

    store
        .create_orchestration(&NewOrchestrationAgentDispatchTask {
            runtime_instance_id: "runtime-a".to_string(),
            runtime_task_id: 7,
            task_ref: "runtime-a:7".to_string(),
            root_task_ref: "runtime-a:1".to_string(),
            parent_task_ref: Some("runtime-a:1".to_string()),
            source_definition_id: Some(alan.id),
            source_node_id: Some(101),
            source_provider: Some("codex".to_string()),
            target_definition_id: bella.id,
            target_node_id: 102,
            original_instruction: "把测试任务交给 Bella".to_string(),
            task_text: "运行测试".to_string(),
            state: AgentDispatchTaskState::Queued,
            tracking: AgentDispatchTracking::Managed,
        })
        .expect("子委派关系应成功持久化");
    let stored = store
        .get_by_task_ref("runtime-a:7")
        .expect("按 task_ref 查询不应失败")
        .expect("已创建的 task_ref 必须命中一条记录");

    assert_eq!(
        stored.runtime_instance_id, "runtime-a",
        "审计记录必须保存创建任务的运行实例身份"
    );
    assert_eq!(
        stored.task_ref, "runtime-a:7",
        "审计记录必须保存 CLI 使用的不透明 task_ref"
    );
    assert_eq!(
        stored.root_task_ref, "runtime-a:1",
        "子任务必须保存根任务引用"
    );
    assert_eq!(
        stored.parent_task_ref.as_deref(),
        Some("runtime-a:1"),
        "第一代子任务必须保存直接父任务引用"
    );
    assert_eq!(
        stored.source_definition_id,
        Some(alan.id),
        "子任务必须保存服务端解析出的来源 Alan definition_id"
    );
    assert_eq!(
        stored.source_node_id,
        Some(101),
        "子任务必须保存服务端验证过的来源 node_id=101"
    );
    assert_eq!(
        stored.definition_id, bella.id,
        "兼容字段 definition_id 必须继续指向目标 Bella"
    );
    assert_eq!(
        stored.node_id, 102,
        "兼容字段 node_id 必须继续指向目标 Bella card"
    );
    assert_eq!(
        stored.state,
        AgentDispatchTaskState::Queued,
        "忙碌 Bella 的子任务审计状态必须真实保存为 Queued"
    );
    assert_eq!(
        stored.tracking,
        AgentDispatchTracking::Managed,
        "可信 Hook 下的子任务必须保存 Managed tracking"
    );
    assert_ne!(
        stored.source_definition_id,
        Some(bella.id),
        "来源 Agent id 不得被目标 Bella id 覆盖"
    );
    assert!(
        !stored.task_ref.is_empty(),
        "CLI 可查询任务的 task_ref 不得为空"
    );
}

/// Agent 子委派后台写入：父子引用与来源信息必须通过同一串行 writer 原样落库。
///
/// 数据构造（含关键数值的推导过程）：
///   runtime_instance_id = "runtime-a"
///   task_ref = "runtime-a:2"
///   parent/root = "runtime-a:1"
///   source_definition/node = 1/101；target_definition/node = 2/202
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建来源和目标两个 Agent Definition。
///   2. 通过 BackgroundAgentDispatchStore enqueue 一条 orchestration 审计记录。
///   3. flush 等待后台 SQLite 写入，再按 task_ref 精确读取。
///
/// 预期结果：
///   - 正断言：来源、目标、父任务、根任务和 Provider 全部保留。
///   - 负断言：后台写入不得降级成 task_ref 为空的旧记录。
#[test]
fn background_dispatch_store_persists_orchestration_lineage_without_foreground_sqlite() {
    let temp = tempfile::tempdir().expect("子委派后台测试应创建临时目录");
    let path = temp.path().join("project.sqlite");
    let definitions = AgentDefinitionStore::new(path.clone());
    let source = definitions
        .create(&NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: String::new(),
        })
        .expect("测试应创建来源 Alan");
    let target = definitions
        .create(&NewAgentDefinition {
            name: "Bella".to_string(),
            runtime_profile: AgentProfile::Claude,
            workflow_description: String::new(),
        })
        .expect("测试应创建目标 Bella");
    let store = AgentDispatchTaskStore::new(path);
    let background = BackgroundAgentDispatchStore::new(store.clone());

    background.create_orchestration(NewOrchestrationAgentDispatchTask {
        runtime_instance_id: "runtime-a".to_string(),
        runtime_task_id: 2,
        task_ref: "runtime-a:2".to_string(),
        root_task_ref: "runtime-a:1".to_string(),
        parent_task_ref: Some("runtime-a:1".to_string()),
        source_definition_id: Some(source.id),
        source_node_id: Some(101),
        source_provider: Some("codex".to_string()),
        target_definition_id: target.id,
        target_node_id: 202,
        original_instruction: "请让 Bella 运行测试".to_string(),
        task_text: "运行测试".to_string(),
        state: AgentDispatchTaskState::Queued,
        tracking: AgentDispatchTracking::Managed,
    });
    background.flush().expect("flush 应等待子委派审计完成");
    let stored = store
        .get_by_task_ref("runtime-a:2")
        .expect("应能查询子委派审计")
        .expect("runtime-a:2 审计记录必须存在");

    assert_eq!(
        stored.source_definition_id,
        Some(source.id),
        "子委派必须保存来源 Agent Definition"
    );
    assert_eq!(
        stored.definition_id, target.id,
        "子委派必须保存目标 Agent Definition"
    );
    assert_eq!(
        stored.parent_task_ref.as_deref(),
        Some("runtime-a:1"),
        "子委派必须保存服务端解析的父任务引用"
    );
    assert_eq!(
        stored.root_task_ref, "runtime-a:1",
        "子委派必须保存根任务引用"
    );
    assert_eq!(
        stored.source_provider.as_deref(),
        Some("codex"),
        "子委派审计必须保存通过鉴权的来源 Provider"
    );
    assert!(
        !stored.task_ref.is_empty(),
        "新编排审计不得降级成 task_ref 为空的旧格式"
    );
}

/// App 重启清理：所有非终态审计行转为 Interrupted，终态记录保持不变。
///
/// 数据构造（含关键数值的推导过程）：
///   非终态 = 4：accepted、queued、submitted、untracked
///   终态 = 1：done
///   总记录 = 5；预期 interrupted ids = 前 4 条
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 写入同一目标的五种状态记录。
///   2. 调用 mark_incomplete_interrupted 模拟新 App 运行实例启动。
///   3. 读取五条记录，逐条核对清理结果。
///
/// 预期结果：
///   - 正断言：accepted/queued/submitted/untracked 全部变为 Interrupted。
///   - 正断言：done 保持 Done，清理返回恰好四个 id。
///   - 负断言：不得恢复任何旧队列状态或修改已完成记录。
#[test]
fn restart_cleanup_interrupts_accepted_queued_submitted_and_untracked_only() {
    let temp = tempfile::tempdir().expect("重启清理测试应创建临时目录");
    let path = temp.path().join("project.sqlite");
    let definition = AgentDefinitionStore::new(path.clone())
        .create(&NewAgentDefinition {
            name: "Bella".to_string(),
            runtime_profile: AgentProfile::Claude,
            workflow_description: String::new(),
        })
        .expect("测试应创建 Bella 定义");
    let store = AgentDispatchTaskStore::new(path);
    for (index, state) in [
        AgentDispatchTaskState::Accepted,
        AgentDispatchTaskState::Queued,
        AgentDispatchTaskState::Submitted,
        AgentDispatchTaskState::Untracked,
        AgentDispatchTaskState::Done,
    ]
    .into_iter()
    .enumerate()
    {
        let task_id = u64::try_from(index + 1).expect("1..=5 可安全转换为 u64");
        store
            .create_orchestration(&NewOrchestrationAgentDispatchTask {
                runtime_instance_id: "runtime-old".to_string(),
                runtime_task_id: task_id,
                task_ref: format!("runtime-old:{task_id}"),
                root_task_ref: format!("runtime-old:{task_id}"),
                parent_task_ref: None,
                source_definition_id: None,
                source_node_id: None,
                source_provider: None,
                target_definition_id: definition.id,
                target_node_id: 102,
                original_instruction: format!("任务-{task_id}"),
                task_text: format!("任务-{task_id}"),
                state,
                tracking: AgentDispatchTracking::Managed,
            })
            .expect("五种状态的审计记录都应成功写入");
    }

    let interrupted = store
        .mark_incomplete_interrupted()
        .expect("App 新运行实例启动时应清理旧非终态记录");
    let tasks = store.list().expect("清理后五条审计记录仍应保留");

    assert_eq!(
        interrupted.len(),
        4,
        "accepted/queued/submitted/untracked 共四条旧任务必须被中断"
    );
    assert_eq!(
        tasks[0].state,
        AgentDispatchTaskState::Interrupted,
        "旧 accepted 任务必须在重启时标记 Interrupted"
    );
    assert_eq!(
        tasks[1].state,
        AgentDispatchTaskState::Interrupted,
        "旧 queued 任务必须在重启时标记 Interrupted"
    );
    assert_eq!(
        tasks[2].state,
        AgentDispatchTaskState::Interrupted,
        "旧 submitted 任务必须在重启时标记 Interrupted"
    );
    assert_eq!(
        tasks[3].state,
        AgentDispatchTaskState::Interrupted,
        "旧 untracked 任务必须在重启时标记 Interrupted"
    );
    assert_eq!(
        tasks[4].state,
        AgentDispatchTaskState::Done,
        "已完成任务在重启清理后必须保持 Done"
    );
    assert_ne!(
        tasks[4].state,
        AgentDispatchTaskState::Interrupted,
        "重启清理不得改写已完成任务"
    );
}
