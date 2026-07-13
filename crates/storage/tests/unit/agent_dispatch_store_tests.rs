use super::super::{
    AgentDefinitionStore, AgentDispatchTaskState, AgentDispatchTaskStore, AgentDispatchTracking,
    NewAgentDefinition, NewAgentDispatchTask,
};
use canvas::AgentProfile;

/// Agent 委派记录应独立保存 FIFO 状态，并在恢复时把所有未完成项标记为 Interrupted。
///
/// 数据构造（含关键数值的推导过程）：
///   definition_id=1（空库首条定义），node_id=101
///   tasks=2：第一项 Submitted、第二项 Queued
///   finish_current 后状态 = Done + Queued
///   submit_next_queued 后状态 = Done + Submitted
///   模拟重启 mark_incomplete_interrupted 后状态 = Done + Interrupted
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在独立 sqlite 中写入同一 Agent 的 Submitted 与 Queued 两项
///   2. finish_current(7, Done) 完成当前项，再 submit_next_queued(7) 推进队首
///   3. mark_incomplete_interrupted 模拟项目重开并读取全部记录
///
/// 预期结果：
///   - 正断言：原始指令、提取任务、definition/node/tracking 均持久化
///   - 正断言：第一项为 Done，第二项最终为 Interrupted
///   - 负断言：已 Done 的第一项不得被恢复逻辑改成 Interrupted
#[test]
fn dispatch_store_persists_fifo_and_interrupts_only_incomplete_tasks() {
    let temp = tempfile::tempdir().expect("测试应创建临时目录");
    let path = temp.path().join("project.sqlite");
    let definition = AgentDefinitionStore::new(path.clone())
        .create(&NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: String::new(),
        })
        .expect("测试应先创建被任务引用的 Alan 定义");
    let store = AgentDispatchTaskStore::new(path);
    store
        .create(&NewAgentDispatchTask {
            definition_id: definition.id,
            node_id: 101,
            original_instruction: "Alan，先修后端，再写测试".to_string(),
            task_text: "修后端".to_string(),
            state: AgentDispatchTaskState::Submitted,
            tracking: AgentDispatchTracking::Managed,
        })
        .expect("第一项 submitted 委派应成功持久化");
    store
        .create(&NewAgentDispatchTask {
            definition_id: definition.id,
            node_id: 101,
            original_instruction: "Alan，先修后端，再写测试".to_string(),
            task_text: "写测试".to_string(),
            state: AgentDispatchTaskState::Queued,
            tracking: AgentDispatchTracking::Managed,
        })
        .expect("第二项 queued 委派应成功持久化");

    let finished = store
        .finish_current(definition.id, AgentDispatchTaskState::Done, None)
        .expect("Done 终态应更新当前 submitted 记录");
    let submitted = store
        .submit_next_queued(definition.id)
        .expect("Done 后应把 FIFO 队首 queued 记录改为 submitted");
    let interrupted = store
        .mark_incomplete_interrupted()
        .expect("模拟重启应中断剩余未完成记录");
    let tasks = store.list().expect("应能读取两条独立 Agent 委派记录");

    assert_eq!(
        finished,
        Some(tasks[0].id),
        "finish_current 应完成第一条记录"
    );
    assert_eq!(submitted, Some(tasks[1].id), "队列推进应提交第二条记录");
    assert_eq!(
        interrupted,
        vec![tasks[1].id],
        "恢复时只应中断第二条未完成记录"
    );
    assert_eq!(tasks.len(), 2, "委派仓库应保留恰好两条任务记录");
    assert_eq!(
        tasks[0].state,
        AgentDispatchTaskState::Done,
        "第一项应保持 Done"
    );
    assert_eq!(
        tasks[1].state,
        AgentDispatchTaskState::Interrupted,
        "第二项在重启恢复时应标记 Interrupted"
    );
    assert_eq!(
        tasks[0].original_instruction, "Alan，先修后端，再写测试",
        "委派记录应保留原始用户指令"
    );
    assert_eq!(tasks[0].task_text, "修后端", "第一项应保留提取后的任务正文");
    assert_eq!(
        tasks[0].definition_id, definition.id,
        "第一项应绑定空库首条 Alan definition_id=1"
    );
    assert_eq!(tasks[0].node_id, 101, "第一项应记录实际 node_id=101");
    assert_eq!(
        tasks[0].tracking,
        AgentDispatchTracking::Managed,
        "Hook 可用的第一项应记录 Managed tracking"
    );
    assert_ne!(
        tasks[0].state,
        AgentDispatchTaskState::Interrupted,
        "已完成第一项不得被恢复逻辑错误中断"
    );
}
