use super::super::{AgentTaskStore, StoredAgentTask};
use agent::{AuditEntry, CardTaskJournal, TaskRecord, TaskState, ToolCallRecord};
use norma::protocol::{CardCommand, CardCommandResult, CardKind, Placement};

/// S-1：AgentTaskStore 应保存任务终态与每次工具调用，并能按 project_id 读取。
///
/// 数据构造：临时 project.sqlite；任务 task_id=7；create 工具结果 node_id=41。
/// 执行过程：追加 queued、running、succeeded 与 tool-call 审计，再 load_project。
/// 预期结果：任务读取为 Succeeded，工具参数/结果完整保留，其他项目不可见。
#[test]
fn agent_task_store_round_trips_task_and_tool_audit() {
    let temp = tempfile::tempdir().expect("test should create temporary project directory");
    let mut store = AgentTaskStore::new(temp.path().join("project.sqlite"));
    store.append(AuditEntry::Task(TaskRecord {
        project_id: "project-a".to_string(),
        task_id: 7,
        intent: "创建终端".to_string(),
        state: TaskState::Queued,
        summary: None,
        error: None,
        completed_tool_calls: 0,
    }));
    store.append(AuditEntry::Task(TaskRecord {
        project_id: "project-a".to_string(),
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
/// 数据构造：同一项目写入 queued task_id=1、running task_id=2、succeeded task_id=3。
/// 执行过程：调用 mark_incomplete_interrupted，再读取项目任务。
/// 预期结果：返回 [1,2]；两条记录变为 Interrupted；成功任务保持不变。
#[test]
fn incomplete_tasks_are_marked_interrupted_without_resuming() {
    let temp = tempfile::tempdir().expect("test should create temporary project directory");
    let mut store = AgentTaskStore::new(temp.path().join("project.sqlite"));
    for (task_id, state) in [
        (1, TaskState::Queued),
        (2, TaskState::Running),
        (3, TaskState::Succeeded),
    ] {
        store.append(AuditEntry::Task(TaskRecord {
            project_id: "project-a".to_string(),
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
        tasks[1].task.state,
        TaskState::Interrupted,
        "running 任务应变为 Interrupted"
    );
    assert_eq!(
        tasks[2].task.state,
        TaskState::Succeeded,
        "已成功任务不得被改写"
    );
}
