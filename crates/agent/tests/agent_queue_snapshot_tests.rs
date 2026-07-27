use agent::{
    AgentAssignment, AgentDispatchSource, AgentQueueTaskSource, AgentRosterEntry,
    AgentTaskCoordinator, AgentTrackingMode,
};
use norma::protocol::AgentProfile;

fn roster(definition_id: i64, name: &str, node_id: u64, busy: bool) -> AgentRosterEntry {
    AgentRosterEntry {
        definition_id,
        name: name.to_string(),
        speech_alias: None,
        profile: AgentProfile::Codex,
        enabled: true,
        active_node_id: Some(node_id),
        tracking: AgentTrackingMode::Managed,
        busy,
    }
}

/// 队列快照：等待数量必须排除当前任务，并保留 FIFO 顺序与真实任务来源。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent = 2：Alan(definition=1,node=101)、Bella(definition=2,node=202)。
///   Alan tasks = current(1) + queued(2) = 3；waiting count = 3 - current(1) = 2。
///   Bella tasks = current(1)，用于建立合法的 Agent 来源任务。
///   Alan waiting order = Norma task(task_id=2) → Bella task(task_id=4)。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Norma 向 Alan 派发当前任务，Alan 进入 busy。
///   2. Norma 再向 Alan 派发第一条等待任务，进入 FIFO。
///   3. Norma 向 Bella 派发父任务，使 Bella 成为合法运行中来源。
///   4. Bella 向 Alan 派发第二条等待任务。
///   5. 读取 Alan 的公开 queue_snapshot。
///
/// 预期结果：
///   - 当前任务独立存在，来源为 Norma，不计入 waiting。
///   - waiting 恰好为 2，顺序保持“补失败测试”在“更新 API 文档”之前。
///   - 第二条 waiting 来源为 Bella(definition_id=2)，不能误报为 Norma 或手动。
#[test]
fn queue_snapshot_excludes_active_and_preserves_fifo_sources() {
    let mut coordinator = AgentTaskCoordinator::new_with_runtime_id(
        "queue-ui",
        vec![
            roster(1, "Alan", 101, false),
            roster(2, "Bella", 202, false),
        ],
    )
    .expect("合法的 Alan/Bella roster 应能创建协调器");

    coordinator
        .dispatch(
            vec![AgentAssignment::new("Alan", "重构用户认证流程")],
            false,
        )
        .expect("Norma 应能向空闲 Alan 派发当前任务");
    coordinator
        .dispatch(
            vec![AgentAssignment::new("Alan", "补充失败路径测试")],
            false,
        )
        .expect("Alan busy 时 Norma 任务应进入等待队列");
    coordinator
        .dispatch(vec![AgentAssignment::new("Bella", "检查认证方案")], false)
        .expect("Norma 应能为 Bella 建立当前父任务");
    coordinator
        .dispatch_from(
            vec![AgentAssignment::new("Alan", "更新 API 文档")],
            false,
            AgentDispatchSource::agent(2, 202),
        )
        .expect("运行中的 Bella 应能向 Alan 派发等待任务");

    let snapshot = coordinator
        .queue_snapshot(1)
        .expect("Alan definition 存在时必须返回队列快照");
    let current = snapshot
        .current
        .expect("Alan 已有当前任务，快照必须单独暴露 current");

    assert_eq!(
        current.task, "重构用户认证流程",
        "current 必须是 Alan 第一条运行任务，不能被等待任务覆盖"
    );
    assert_eq!(
        current.source,
        AgentQueueTaskSource::Norma,
        "Norma 顶层派发的当前任务来源必须标记为 Norma"
    );
    assert_eq!(
        snapshot.waiting.len(),
        2,
        "waiting 数量必须只统计等待任务：3 个 Alan tasks - 1 个 current = 2"
    );
    assert_eq!(
        snapshot.waiting[0].task, "补充失败路径测试",
        "FIFO 第一条等待任务必须保持 Norma 最先入队的任务"
    );
    assert_eq!(
        snapshot.waiting[0].source,
        AgentQueueTaskSource::Norma,
        "第一条等待任务由 Norma 派发，来源不得误报为手动"
    );
    assert_eq!(
        snapshot.waiting[1].task, "更新 API 文档",
        "FIFO 第二条等待任务必须保持 Bella 后入队的任务"
    );
    assert_eq!(
        snapshot.waiting[1].source,
        AgentQueueTaskSource::Agent { definition_id: 2 },
        "Bella 派发的等待任务必须保留 source definition_id=2"
    );
    assert!(
        snapshot
            .waiting
            .iter()
            .all(|task| task.task != current.task),
        "current 不得重复出现在 waiting，否则标题栏 QUEUE 数量会错误包含运行项"
    );
    assert_ne!(
        snapshot.waiting[1].source,
        AgentQueueTaskSource::Manual,
        "Agent 来源任务不得被误标为用户直接输入"
    );
}
