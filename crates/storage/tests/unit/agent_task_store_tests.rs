use super::super::{AgentTaskStore, StoredAgentTask};
use agent::{AuditEntry, CardTaskJournal, TaskRecord, TaskState, ToolCallRecord};
use norma::protocol::{CardCommand, CardCommandResult, CardKind, Placement};

/// S-1：AgentTaskStore 应保存任务终态与每次工具调用，并能按 project_id 读取。
///
/// 数据构造（含关键数值的推导过程）：
///   project tasks    = 1：project-a/task_id=7
///   state writes     = 2：Queued + Succeeded
///   tool call writes = 1：Create 返回 node_id=41
///   trace ids        = 1：两次状态写入共享 trace-project-a-turn-1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 写入同一任务的 Queued 与 Succeeded 状态，并保持 TraceId
///   2. 写入一次 Create 工具调用与 Created 回执
///   3. 分别读取 project-a 与 project-b 的任务历史
///
/// 预期结果：
///   - 正断言：project-a 任务、TraceId、终态与工具回执完整保留
///   - 负断言：project-b 不得读取 project-a 的任何任务
#[test]
fn agent_task_store_round_trips_task_and_tool_audit() {
    let temp = tempfile::tempdir().expect("test should create temporary project directory");
    let mut store = AgentTaskStore::new(temp.path().join("project.sqlite"));
    store.append(AuditEntry::Task(TaskRecord {
        project_id: "project-a".to_string(),
        trace_id: "trace-project-a-turn-1".to_string(),
        task_id: 7,
        intent: "创建终端".to_string(),
        state: TaskState::Queued,
        summary: None,
        error: None,
        completed_tool_calls: 0,
    }));
    store.append(AuditEntry::Task(TaskRecord {
        project_id: "project-a".to_string(),
        trace_id: "trace-project-a-turn-1".to_string(),
        task_id: 7,
        intent: "创建终端".to_string(),
        state: TaskState::Succeeded,
        summary: Some("创建终端".to_string()),
        error: None,
        completed_tool_calls: 1,
    }));
    store.append(AuditEntry::ToolCall(ToolCallRecord {
        project_id: "project-a".to_string(),
        task_id: 7,
        tool_call_id: "call-1".to_string(),
        command: CardCommand::Create {
            kind: CardKind::Terminal { shell: None },
            placement: Placement::VisibleArea,
        },
        result: Ok(CardCommandResult::Created { node_id: 41 }),
    }));

    let project_tasks = store
        .load_project("project-a")
        .expect("project task audit should load");
    let other_project = store
        .load_project("project-b")
        .expect("other project audit query should succeed");

    assert_eq!(project_tasks.len(), 1, "project-a 应只读到一个任务记录");
    assert_eq!(
        other_project.len(),
        0,
        "project-b 不得读到 project-a 的任务"
    );
    let StoredAgentTask {
        task,
        tool_calls,
        tool_errors,
    } = &project_tasks[0];
    assert_eq!(
        task.state,
        TaskState::Succeeded,
        "任务终态应持久化为 Succeeded"
    );
    assert_eq!(
        task.trace_id, "trace-project-a-turn-1",
        "任务终态更新后必须保留最初用户意图的 TraceId"
    );
    assert_eq!(task.completed_tool_calls, 1, "已完成工具数应持久化");
    assert_eq!(tool_calls.len(), 1, "每次工具调用都应保留一条审计记录");
    assert!(
        tool_errors.is_empty(),
        "无效工具调用错误表不应混入成功工具记录"
    );
    assert_eq!(
        tool_calls[0].tool_call_id, "call-1",
        "tool call id 应保持稳定"
    );
}

/// S-2：项目重新打开时，Queued/Running 任务必须标记 Interrupted，不能自动续跑。
///
/// 数据构造（含关键数值的推导过程）：
///   incomplete tasks = 2：Queued task_id=1 + Running task_id=2
///   terminal tasks   = 2：Succeeded task_id=3 + PartiallyDispatched task_id=4
///   total tasks      = 2 + 2 = 4
///   trace ids        = 4：每个独立历史任务各一个稳定 TraceId
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 写入四条不同生命周期状态的任务
///   2. 调用 mark_incomplete_interrupted 模拟项目重开清理
///   3. 重新读取四条历史任务并核对状态与 TraceId
///
/// 预期结果：
///   - 正断言：task_id=1、2 进入 Interrupted 且原 TraceId 不变
///   - 正断言：task_id=3、4 分别保持 Succeeded 与 PartiallyDispatched
///   - 负断言：两种派发终态都不得被重开清理改写为 Interrupted
#[test]
fn incomplete_tasks_are_marked_interrupted_without_resuming() {
    let temp = tempfile::tempdir().expect("test should create temporary project directory");
    let mut store = AgentTaskStore::new(temp.path().join("project.sqlite"));
    for (task_id, state) in [
        (1, TaskState::Queued),
        (2, TaskState::Running),
        (3, TaskState::Succeeded),
        (4, TaskState::PartiallyDispatched),
    ] {
        store.append(AuditEntry::Task(TaskRecord {
            project_id: "project-a".to_string(),
            trace_id: format!("trace-project-a-turn-{task_id}"),
            task_id,
            intent: format!("task-{task_id}"),
            state,
            summary: None,
            error: None,
            completed_tool_calls: 0,
        }));
    }

    let interrupted = store
        .mark_incomplete_interrupted("project-a")
        .expect("incomplete task recovery should succeed");
    let tasks = store
        .load_project("project-a")
        .expect("recovered tasks should load");

    assert_eq!(interrupted, vec![1, 2], "只有 queued/running 任务应被中断");
    assert_eq!(
        tasks[0].task.state,
        TaskState::Interrupted,
        "queued 任务应变为 Interrupted"
    );
    assert_eq!(
        tasks[0].task.trace_id, "trace-project-a-turn-1",
        "Queued 任务转 Interrupted 后必须保留原 TraceId"
    );
    assert_eq!(
        tasks[1].task.state,
        TaskState::Interrupted,
        "running 任务应变为 Interrupted"
    );
    assert_eq!(
        tasks[1].task.trace_id, "trace-project-a-turn-2",
        "Running 任务转 Interrupted 后必须保留原 TraceId"
    );
    assert_eq!(
        tasks[2].task.state,
        TaskState::Succeeded,
        "已成功任务不得被改写"
    );
    assert_eq!(
        tasks[2].task.trace_id, "trace-project-a-turn-3",
        "已成功任务的 TraceId 不得在清理期间被改写"
    );
    assert_eq!(
        tasks[3].task.state,
        TaskState::PartiallyDispatched,
        "部分派发终态不得被项目重开清理改写"
    );
    assert_eq!(
        tasks[3].task.trace_id, "trace-project-a-turn-4",
        "部分派发任务的 TraceId 不得在清理期间被改写"
    );
}
