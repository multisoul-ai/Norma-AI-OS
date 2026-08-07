use super::super::{
    AgentDefinitionStore, AgentDispatchTaskState, AgentDispatchTaskStore, AgentDispatchTracking,
    BackgroundAgentDispatchStore, NewAgentDefinition, NewAgentDispatchTask,
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
            runtime_task_id: 1,
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
            runtime_task_id: 2,
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

/// Agent 任务取消审计：排队任务立即 Cancelled，运行任务先 CancelRequested 再由终止确认收口。
///
/// 数据构造（含关键数值的推导过程）：
///   definition_id = 1（空库首条 Alan 定义）
///   tasks = 2（第一项 Submitted + 第二项 Queued）
///   cancellation writes = 2（running=true + running=false）
///   terminal confirmation = 1（CancelRequested → Cancelled）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建一条运行中任务与一条排队任务
///   2. 按定义和任务正文分别持久化运行取消请求与排队立即取消
///   3. 模拟 AgentCard 确认停止，将当前 CancelRequested 收口为 Cancelled
///
/// 预期结果：
///   - 正断言：两条记录最终都为 Cancelled，且更新命中各自准确 id
///   - 负断言：运行任务不能停留在 Submitted，排队任务不能停留在 Queued
#[test]
fn dispatch_store_persists_requested_and_confirmed_cancellation_states() {
    let temp = tempfile::tempdir().expect("取消审计测试应创建临时目录");
    let path = temp.path().join("project.sqlite");
    let definition = AgentDefinitionStore::new(path.clone())
        .create(&NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: String::new(),
        })
        .expect("取消审计测试应先创建 Alan 定义");
    let store = AgentDispatchTaskStore::new(path);
    let running = store
        .create(&NewAgentDispatchTask {
            runtime_task_id: 1,
            definition_id: definition.id,
            node_id: 101,
            original_instruction: "运行第一项".to_string(),
            task_text: "运行第一项".to_string(),
            state: AgentDispatchTaskState::Submitted,
            tracking: AgentDispatchTracking::Managed,
        })
        .expect("运行任务应成功持久化");
    let queued = store
        .create(&NewAgentDispatchTask {
            runtime_task_id: 2,
            definition_id: definition.id,
            node_id: 101,
            original_instruction: "排队第二项".to_string(),
            task_text: "排队第二项".to_string(),
            state: AgentDispatchTaskState::Queued,
            tracking: AgentDispatchTracking::Managed,
        })
        .expect("排队任务应成功持久化");

    let requested = store
        .persist_cancellation(running.runtime_task_id, true)
        .expect("运行任务应写入 cancel_requested");
    let cancelled_queued = store
        .persist_cancellation(queued.runtime_task_id, false)
        .expect("排队任务应立即写入 cancelled");
    let confirmed = store
        .finish_current(definition.id, AgentDispatchTaskState::Cancelled, None)
        .expect("AgentCard 终止确认应把当前取消请求收口");
    let tasks = store.list().expect("取消后应能读取两条审计记录");

    assert_eq!(
        requested,
        Some(running.id),
        "running cancellation must update exactly the submitted task"
    );
    assert_eq!(
        cancelled_queued,
        Some(queued.id),
        "queued cancellation must update exactly the queued task"
    );
    assert_eq!(
        confirmed,
        Some(running.id),
        "terminal confirmation must close the same running cancellation record"
    );
    assert_eq!(
        tasks[0].state,
        AgentDispatchTaskState::Cancelled,
        "the confirmed running task must end as Cancelled"
    );
    assert_eq!(
        tasks[1].state,
        AgentDispatchTaskState::Cancelled,
        "the removed queued task must immediately remain Cancelled"
    );
    assert_ne!(
        tasks[0].state,
        AgentDispatchTaskState::Submitted,
        "a cancellation request must not leave the running audit row as Submitted"
    );
    assert_ne!(
        tasks[1].state,
        AgentDispatchTaskState::Queued,
        "an immediately cancelled queued audit row must not remain Queued"
    );
}

/// Agent 任务取消必须按 runtime task id 精确命中，不能误取消正文相同的相邻任务。
///
/// 数据构造（含关键数值的推导过程）：
///   identical queued tasks = 2（runtime_task_id 分别为 2、3）
///   cancellation writes = 1（只取消 runtime_task_id=3）
///   expected states = Queued + Cancelled
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 为同一个 Alan 写入两条正文完全相同的排队任务
///   2. 仅用第二条的稳定 runtime_task_id=3 请求取消
///   3. 读取两条审计记录，逐一核对 id 与状态
///
/// 预期结果：
///   - 正断言：runtime_task_id=3 的第二条记录变为 Cancelled
///   - 负断言：runtime_task_id=2 的第一条记录仍是 Queued，没有因正文相同被误取消
#[test]
fn cancellation_uses_runtime_task_id_when_task_text_is_duplicated() {
    let temp = tempfile::tempdir().expect("重复正文取消测试应创建临时目录");
    let path = temp.path().join("project.sqlite");
    let definition = AgentDefinitionStore::new(path.clone())
        .create(&NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: String::new(),
        })
        .expect("重复正文取消测试应先创建 Alan 定义");
    let store = AgentDispatchTaskStore::new(path);
    for runtime_task_id in [2, 3] {
        store
            .create(&NewAgentDispatchTask {
                runtime_task_id,
                definition_id: definition.id,
                node_id: 101,
                original_instruction: "检查当前项目".to_string(),
                task_text: "检查当前项目".to_string(),
                state: AgentDispatchTaskState::Queued,
                tracking: AgentDispatchTracking::Managed,
            })
            .expect("两条正文相同的排队任务都应成功持久化");
    }

    let cancelled = store
        .persist_cancellation(3, false)
        .expect("runtime_task_id=3 应精确取消第二条记录");
    let tasks = store.list().expect("精确取消后应能读取两条审计记录");

    assert_eq!(tasks.len(), 2, "重复正文测试应保留恰好两条独立记录");
    assert_eq!(
        tasks[0].runtime_task_id, 2,
        "第一条必须保留 runtime_task_id=2"
    );
    assert_eq!(
        tasks[0].state,
        AgentDispatchTaskState::Queued,
        "未指定的 runtime_task_id=2 必须仍在队列中"
    );
    assert_eq!(
        tasks[1].runtime_task_id, 3,
        "第二条必须保留 runtime_task_id=3"
    );
    assert_eq!(
        tasks[1].state,
        AgentDispatchTaskState::Cancelled,
        "指定的 runtime_task_id=3 必须变为 Cancelled"
    );
    assert_eq!(
        cancelled,
        Some(tasks[1].id),
        "取消操作必须返回第二条记录的数据库 id"
    );
    assert_ne!(
        cancelled,
        Some(tasks[0].id),
        "取消操作不得命中正文相同的第一条记录"
    );
}

/// Agent 委派后台写入器必须按发送顺序串行落库，避免终态越过创建记录。
///
/// 数据构造（含关键数值的推导过程）：
///   definition_id = 1（空库首条定义）
///   channel commands = Create(Submitted) + Create(Queued) + Finish(Done) + SubmitNext，共 4 条
///   expected rows = 2：第一条 Done、第二条 Submitted
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建项目定义和独立 BackgroundAgentDispatchStore worker；
///   2. 不等待地连续发送两条创建、当前完成、队首提交命令；
///   3. flush barrier 等待此前 4 条命令按 FIFO 执行；
///   4. 通过公开 store interface 读取最终状态。
///
/// 预期结果：
///   - 正断言：第一项为 Done，第二项为 Submitted；
///   - 负断言：第二项不得仍为 Queued，证明终态转换没有越过异步创建。
#[test]
fn background_dispatch_store_serializes_create_and_terminal_transitions() {
    let temp = tempfile::tempdir().expect("后台委派测试应创建临时目录");
    let path = temp.path().join("project.sqlite");
    let definition = AgentDefinitionStore::new(path.clone())
        .create(&NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: String::new(),
        })
        .expect("后台委派测试应先创建 Alan 定义");
    let store = AgentDispatchTaskStore::new(path);
    let background = BackgroundAgentDispatchStore::new(store.clone());

    background.create(NewAgentDispatchTask {
        runtime_task_id: 1,
        definition_id: definition.id,
        node_id: 101,
        original_instruction: "运行第一项".to_string(),
        task_text: "运行第一项".to_string(),
        state: AgentDispatchTaskState::Submitted,
        tracking: AgentDispatchTracking::Managed,
    });
    background.create(NewAgentDispatchTask {
        runtime_task_id: 2,
        definition_id: definition.id,
        node_id: 101,
        original_instruction: "运行第二项".to_string(),
        task_text: "运行第二项".to_string(),
        state: AgentDispatchTaskState::Queued,
        tracking: AgentDispatchTracking::Managed,
    });
    background.finish_current(definition.id, AgentDispatchTaskState::Done, None);
    background.submit_next_queued(definition.id);
    background
        .flush()
        .expect("flush barrier 应等待此前后台命令全部完成");
    let tasks = store.list().expect("flush 后应能读取后台写入结果");

    assert_eq!(tasks.len(), 2, "后台 writer 应持久化恰好两条委派记录");
    assert_eq!(
        tasks[0].state,
        AgentDispatchTaskState::Done,
        "第一条 Submitted 应先创建，再被终态命令更新为 Done"
    );
    assert_eq!(
        tasks[1].state,
        AgentDispatchTaskState::Submitted,
        "第二条 Queued 应在第一条完成后按 FIFO 更新为 Submitted"
    );
    assert_ne!(
        tasks[1].state,
        AgentDispatchTaskState::Queued,
        "flush 后第二条不得仍为 Queued，否则后台命令发生乱序或丢失"
    );
}

/// Agent 委派后台 writer 切换项目时，必须先完成旧 writer 的中断再接受新写入。
///
/// 数据构造（含关键数值的推导过程）：
///   old writer rows = 1 条 Submitted
///   new writer rows = 同一数据库新增 1 条 Submitted
///   expected final rows = 2 条：旧 Interrupted + 新 Submitted
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 旧 writer 创建一条 Submitted，flush 确认已落库；
///   2. 以旧 writer 为 predecessor 创建新 writer，并立即 enqueue 新任务；
///   3. 新 writer 后台等待旧 writer 执行 MarkIncompleteInterrupted 与 flush；
///   4. 新 writer 再恢复数据库、创建新任务并 flush。
///
/// 预期结果：
///   - 正断言：旧任务为 Interrupted，新任务保持 Submitted；
///   - 负断言：旧 writer 的迟到中断不得把新任务也改为 Interrupted。
#[test]
fn background_dispatch_store_handoff_orders_old_interrupt_before_new_create() {
    let temp = tempfile::tempdir().expect("writer handoff 测试应创建临时目录");
    let path = temp.path().join("project.sqlite");
    let definition = AgentDefinitionStore::new(path.clone())
        .create(&NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: String::new(),
        })
        .expect("writer handoff 测试应先创建 Alan 定义");
    let store = AgentDispatchTaskStore::new(path);
    let old_writer = BackgroundAgentDispatchStore::new(store.clone());
    old_writer.create(NewAgentDispatchTask {
        runtime_task_id: 1,
        definition_id: definition.id,
        node_id: 101,
        original_instruction: "旧任务".to_string(),
        task_text: "旧任务".to_string(),
        state: AgentDispatchTaskState::Submitted,
        tracking: AgentDispatchTracking::Managed,
    });
    old_writer.flush().expect("旧任务应先落库");

    let new_writer = BackgroundAgentDispatchStore::new_after(Some(old_writer), store.clone());
    new_writer.create(NewAgentDispatchTask {
        runtime_task_id: 2,
        definition_id: definition.id,
        node_id: 101,
        original_instruction: "新任务".to_string(),
        task_text: "新任务".to_string(),
        state: AgentDispatchTaskState::Submitted,
        tracking: AgentDispatchTracking::Managed,
    });
    new_writer.flush().expect("新 writer 应完成有序 handoff");
    let tasks = store.list().expect("handoff 后应能读取两条任务");

    assert_eq!(tasks.len(), 2, "handoff 前后应各保留一条审计记录");
    assert_eq!(
        tasks[0].state,
        AgentDispatchTaskState::Interrupted,
        "旧 writer 的未完成任务应在交接时中断"
    );
    assert_eq!(
        tasks[1].state,
        AgentDispatchTaskState::Submitted,
        "新任务必须在旧中断完成后创建并保持 Submitted"
    );
    assert_ne!(
        tasks[1].state,
        AgentDispatchTaskState::Interrupted,
        "旧 writer 的迟到命令不得污染新任务"
    );
}
