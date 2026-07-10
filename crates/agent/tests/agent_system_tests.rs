mod support;

use agent::{
    AgentCommand, AgentEvent, AgentSystem, CardTaskBudget, MemoryJournal, ProviderResponse,
};
use norma::protocol::CardCommand;
use support::{
    init_trace, response, system, task_id_from, text_response, tool, RecordingHost,
    ScriptedProvider,
};

/// A-1：普通聊天只产生文本事件，不创建 CardTask，也不触碰 CardToolHost。
///
/// 数据构造：Dialogue Provider 返回一条普通文本，不返回 card_task 工具调用。
/// 执行过程：提交一个 UserTurn，观察 AgentEvent、任务列表和 host 命令记录。
/// 预期结果：有且仅有一条文本回复；任务数和卡片副作用均为零。
#[test]
fn ordinary_chat_does_not_dispatch_card_task() {
    let mut system = system(
        ScriptedProvider::new([text_response("今天天气不错")]),
        ScriptedProvider::default(),
        RecordingHost::default(),
    );

    let events = system
        .submit(AgentCommand::UserTurn("你好".to_string()))
        .expect("普通聊天应成功交给 Dialogue Provider");

    assert_eq!(
        events,
        vec![AgentEvent::DialogueTextDelta {
            text: "今天天气不错".to_string(),
        }],
        "普通聊天应只产生文本事件，不应伪造任务回执"
    );
    assert_eq!(system.pending_task_count(), 0, "普通聊天不得创建 CardTask");
    assert!(
        system.card_tool_host().commands.is_empty(),
        "普通聊天不得调用任何卡片工具"
    );
}

/// A-1b：模型同时返回 card_task start 与“已完成”文本时，系统不得把未执行的承诺播给用户。
///
/// 数据构造：同一 Dialogue response 含两次 start 与一段虚假的完成文本。
/// 执行过程：提交单个 UserTurn，观察文本事件和 queued 任务数量。
/// 预期结果：只保留一个任务；不产生 DialogueTextDelta，避免未执行的成功声明。
#[test]
fn dialogue_task_start_suppresses_premature_completion_text_and_duplicates() {
    let dialogue = ScriptedProvider::new([ProviderResponse {
        text: "已经完成创建".to_string(),
        tool_calls: vec![
            tool(
                "dialogue-call-1",
                "card_task",
                r#"{"action":"start","intent":"创建终端"}"#,
            ),
            tool(
                "dialogue-call-2",
                "card_task",
                r#"{"action":"start","intent":"再创建一个终端"}"#,
            ),
        ],
    }]);
    let mut system = system(
        dialogue,
        ScriptedProvider::default(),
        RecordingHost::default(),
    );

    let events = system
        .submit(AgentCommand::UserTurn("创建终端".to_string()))
        .expect("带 card_task 的 Dialogue turn 应成功处理");

    assert_eq!(
        system.pending_task_count(),
        1,
        "一个用户 turn 最多只应派发一个任务"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AgentEvent::DialogueTextDelta { .. })),
        "任务尚未进入终态时不得播报模型虚假的完成文本"
    );
}

/// A-2：Dialogue 的 card_task start 只产生 queued receipt；CardTask 在后续 pump 中独立运行。
///
/// 数据构造：Dialogue 返回一个 start intent；Subagent 依次返回 create、send_text、最终文本。
/// 执行过程：先提交 UserTurn，再逐次 pump，确保 queued 时尚无底层工具调用。
/// 预期结果：task id 在 queued/running/succeeded 事件和 Journal 中一致，send_text 指向新节点。
#[test]
fn dialogue_start_is_queued_before_independent_subagent_execution() {
    init_trace();
    let dialogue = ScriptedProvider::new([response([tool(
        "dialogue-call-1",
        "card_task",
        r#"{"action":"start","intent":"创建 Codex 并发送分析 README"}"#,
    )])]);
    let card = ScriptedProvider::new([
        response([tool(
            "card-call-1",
            "create_card",
            r#"{"kind":"agent","profile":"codex"}"#,
        )]),
        response([tool(
            "card-call-2",
            "send_text",
            r#"{"node_id":1,"text":"分析 README","submit":true}"#,
        )]),
        text_response("已完成卡片操作"),
    ]);
    let mut system = system(dialogue, card, RecordingHost::default());

    let queued = system
        .submit(AgentCommand::UserTurn(
            "创建 Codex 并发送分析 README".to_string(),
        ))
        .expect("Dialogue start 应返回 queued receipt");
    let task_id = queued
        .iter()
        .find_map(|event| match event {
            AgentEvent::TaskQueued { task_id, .. } => Some(*task_id),
            _ => None,
        })
        .expect("start 必须产生 TaskQueued");
    assert_eq!(
        system.pending_task_count(),
        1,
        "queued receipt 后任务应进入队列"
    );
    assert!(
        system.card_tool_host().commands.is_empty(),
        "queued receipt 返回前不得执行底层卡片工具"
    );

    let mut observed = Vec::new();
    // 6 = 启动并取首轮响应 + 执行 create + 取第二轮响应 + 执行 send + 取终轮文本 + 一次空闲保护 tick。
    for _ in 0..6 {
        observed.extend(system.pump());
    }

    assert!(
        observed.contains(&AgentEvent::TaskSucceeded {
            task_id,
            summary: "已完成卡片操作".to_string(),
        }),
        "只有 Subagent tool loop 完成后才能产生 Succeeded"
    );
    assert_eq!(
        system.card_tool_host().commands.len(),
        2,
        "同一 CardTask 应完成 create 与 send_text 两次底层调用"
    );
    assert_eq!(
        system.card_tool_host().commands[1].command,
        CardCommand::SendText {
            node_id: 1,
            text: "分析 README".to_string(),
            submit: true,
        },
        "send_text 必须指向本任务创建的节点并保留 submit=true"
    );
    assert!(
        system.journal().entries().iter().any(|entry| {
            entry.task_id() == task_id && entry.tool_call_id() == Some("card-call-2")
        }),
        "每次工具调用都必须写入带 task id 的审计记录"
    );
}

/// A-3：重复 tool call id 至多执行一次，取消只阻止尚未开始的调用且保留已有副作用。
///
/// 数据构造：Subagent 第一个响应重复两次相同 id 的 create，随后才返回文本。
/// 执行过程：pump 完成首个调用后请求取消，再继续 pump。
/// 预期结果：host 只执行一次 create，任务进入带 partial effects 的取消终态。
#[test]
fn cancellation_is_non_rollback_and_tool_calls_are_idempotent() {
    let dialogue = ScriptedProvider::new([response([tool(
        "dialogue-call-1",
        "card_task",
        r#"{"action":"start","intent":"创建终端并提交 pwd"}"#,
    )])]);
    let duplicate_create = response([
        tool("same-call", "create_card", r#"{"kind":"terminal"}"#),
        tool("same-call", "create_card", r#"{"kind":"terminal"}"#),
    ]);
    let mut system = system(
        dialogue,
        ScriptedProvider::new([duplicate_create, text_response("不会到达")]),
        RecordingHost::default(),
    );
    let queued = system
        .submit(AgentCommand::UserTurn("创建终端并提交 pwd".to_string()))
        .expect("任务应成功入队");
    let task_id = queued
        .iter()
        .find_map(|event| match event {
            AgentEvent::TaskQueued { task_id, .. } => Some(*task_id),
            _ => None,
        })
        .expect("应能取得 task id");

    let _ = system.pump();
    let _ = system.pump();
    let cancel = system
        .submit(AgentCommand::CancelTask { task_id })
        .expect("取消运行中任务应成功登记");
    assert!(
        cancel
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskCancelRequested { .. })),
        "取消运行中任务应先产生取消请求事件"
    );
    let terminal = system.pump();

    assert_eq!(
        system.card_tool_host().commands.len(),
        1,
        "重复 tool call id 和取消都不得重复执行或开启新的副作用"
    );
    assert!(
        terminal.iter().any(|event| matches!(
            event,
            AgentEvent::TaskCancelled {
                partial_effects: true,
                ..
            }
        )),
        "已有创建副作用的取消必须明确标记 partial_effects"
    );
}

/// A-4：第二个任务必须等第一个任务终态后才可调用底层工具。
///
/// 数据构造：连续提交两个 DirectCardTask，两个 Subagent 各需一个 create 调用。
/// 执行过程：逐次 pump 并检查 host 的调用数量与任务终态。
/// 预期结果：第二个任务在第一个成功前保持 queued，绝不并行执行。
#[test]
fn card_tasks_are_serial_per_project() {
    let mut system = system(
        ScriptedProvider::default(),
        ScriptedProvider::new([
            response([tool("one", "create_card", r#"{"kind":"terminal"}"#)]),
            text_response("one done"),
            response([tool("two", "create_card", r#"{"kind":"terminal"}"#)]),
            text_response("two done"),
        ]),
        RecordingHost::default(),
    );
    let first = system
        .submit(AgentCommand::DirectCardTask("创建第一个终端".to_string()))
        .expect("第一个 DirectCardTask 应入队");
    let second = system
        .submit(AgentCommand::DirectCardTask("创建第二个终端".to_string()))
        .expect("第二个 DirectCardTask 应入队");
    let first_id = task_id_from(&first);
    let second_id = task_id_from(&second);

    assert_eq!(
        system.pending_task_count(),
        2,
        "连续派发的两个任务都应先排队"
    );
    let first_tick = system.pump();
    assert!(
        first_tick.contains(&AgentEvent::TaskRunning { task_id: first_id }),
        "首个 pump 只能启动队首任务"
    );
    assert_eq!(
        system.card_tool_host().commands.len(),
        0,
        "启动任务时不应误执行尚未取得的工具调用"
    );
    let second_tick = system.pump();
    assert_eq!(
        system.card_tool_host().commands.len(),
        1,
        "第二个 pump 应只执行第一个任务的首个工具调用"
    );
    assert!(
        !second_tick.contains(&AgentEvent::TaskRunning { task_id: second_id }),
        "第一个任务未结束前不得启动第二个任务"
    );
    // 4 = 完成首任务的剩余响应/工具/终态，再启动第二任务并至少执行一轮。
    for _ in 0..4 {
        let _ = system.pump();
    }
    assert!(
        system
            .events()
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskSucceeded { task_id, .. } if *task_id == second_id)),
        "队首任务完成后第二个任务最终应被执行并成功"
    );
}

/// A-5：任一硬预算达到上限都必须失败，并保留已经完成的副作用与审计。
///
/// 数据构造：max_tool_calls=1；Subagent 一轮返回两个 create_card tool call。
/// 执行过程：pump 启动/取响应、执行首个调用，再尝试执行第二个调用。
/// 预期结果：只执行一次 host，任务以 BudgetExceeded 失败，不假装成功。
#[test]
fn budget_exceeded_stops_before_next_tool_call() {
    let dialogue = ScriptedProvider::new([response([tool(
        "dialogue-call-1",
        "card_task",
        r#"{"action":"start","intent":"创建两个终端"}"#,
    )])]);
    let card = ScriptedProvider::new([response([
        tool("card-call-1", "create_card", r#"{"kind":"terminal"}"#),
        tool("card-call-2", "create_card", r#"{"kind":"terminal"}"#),
    ])]);
    let mut budget = CardTaskBudget::default();
    budget.max_tool_calls = 1;
    let mut system = AgentSystem::new(
        "project-a",
        dialogue,
        card,
        RecordingHost::default(),
        MemoryJournal::default(),
        budget,
    );
    let queued = system
        .submit(AgentCommand::UserTurn("创建两个终端".to_string()))
        .expect("任务应成功入队");
    let task_id = task_id_from(&queued);

    let _ = system.pump();
    let _ = system.pump();
    let failed = system.pump();

    assert_eq!(
        system.card_tool_host().commands.len(),
        1,
        "达到 max_tool_calls=1 后不得执行第二个工具调用"
    );
    assert!(
        failed.iter().any(|event| matches!(
            event,
            AgentEvent::TaskFailed { task_id: id, reason }
                if *id == task_id && reason.contains("BudgetExceeded")
        )),
        "预算耗尽必须产生真实失败终态"
    );
}

/// A-6：项目关闭会中断活动和排队任务，之后 pump 不得再触碰 CardToolHost。
///
/// 数据构造：两个 DirectCardTask，首任务已有 Subagent 工具响应，第二任务仍在队列。
/// 执行过程：启动首任务后发送 ProjectClosed，再继续 pump。
/// 预期结果：两个任务均 Interrupted，host 调用数保持为零。
#[test]
fn project_close_interrupts_active_and_pending_tasks() {
    let mut system = AgentSystem::new(
        "project-a",
        ScriptedProvider::default(),
        ScriptedProvider::new([response([tool(
            "card-call-1",
            "create_card",
            r#"{"kind":"terminal"}"#,
        )])]),
        RecordingHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );
    let first = system
        .submit(AgentCommand::DirectCardTask("任务一".to_string()))
        .expect("任务一应入队");
    let second = system
        .submit(AgentCommand::DirectCardTask("任务二".to_string()))
        .expect("任务二应入队");
    let first_id = task_id_from(&first);
    let second_id = task_id_from(&second);
    let _ = system.pump();

    let interrupted = system
        .submit(AgentCommand::ProjectClosed)
        .expect("项目关闭应能中断任务");
    let _ = system.pump();

    assert_eq!(
        system.card_tool_host().commands.len(),
        0,
        "项目关闭后不得开始任何卡片工具副作用"
    );
    assert!(
        interrupted.iter().any(|event| matches!(
            event,
            AgentEvent::TaskInterrupted { task_id } if *task_id == first_id
        )),
        "活动任务应被标记 Interrupted"
    );
    assert!(
        interrupted.iter().any(|event| matches!(
            event,
            AgentEvent::TaskInterrupted { task_id } if *task_id == second_id
        )),
        "排队任务应被标记 Interrupted"
    );
}

/// A-7：模型给出非法工具参数时也必须记录 tool call 与错误，且不得产生卡片副作用。
///
/// 数据构造：Subagent 返回未注册的 `rename_card` 调用；host 不实现该工具。
/// 执行过程：启动任务、读取响应、执行该调用，再查询 MemoryJournal。
/// 预期结果：host 调用数为零，ToolError 审计包含稳定 call id 和原始参数。
#[test]
fn invalid_tool_call_is_audited_without_card_side_effect() {
    let mut system = AgentSystem::new(
        "project-a",
        ScriptedProvider::default(),
        ScriptedProvider::new([response([tool(
            "bad-call",
            "rename_card",
            r#"{"node_id":1,"title":"new"}"#,
        )])]),
        RecordingHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );
    system
        .submit(AgentCommand::DirectCardTask("改名".to_string()))
        .expect("任务应成功入队");
    let _ = system.pump();
    let _ = system.pump();
    let _ = system.pump();

    assert!(
        system.card_tool_host().commands.is_empty(),
        "非法工具调用不得抵达 CardToolHost"
    );
    assert!(
        system.journal().entries().iter().any(|entry| matches!(
            entry,
            agent::AuditEntry::ToolError(error)
                if error.tool_call_id == "bad-call"
                    && error.tool_name == "rename_card"
                    && error.arguments_json.contains("node_id")
        )),
        "非法工具调用也必须写入包含原始参数的错误审计"
    );
}
