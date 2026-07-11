mod support;

use agent::{AgentCommand, AgentEvent};
use support::{system, text_response, RecordingHost, ScriptedProvider};

/// DP-1：明确且信息充分的创建卡片指令在模型只追问确认时必须确定性入队。
///
/// 数据构造：Dialogue Provider 返回“请确认终端类型”的纯文本，不产生任何工具调用。
/// 执行过程：提交“在画布中创建一个终端卡片”这一明确、可直接执行的用户指令。
/// 预期结果：丢弃确认文本并产生且只产生一个 TaskQueued；尚未 pump 时画布无副作用。
#[test]
fn explicit_create_terminal_instruction_queues_when_model_only_asks_for_confirmation() {
    let dialogue = ScriptedProvider::new([text_response("请确认要使用哪一种终端")]);
    let mut agent_system = system(
        dialogue,
        ScriptedProvider::default(),
        RecordingHost::default(),
    );

    let events = agent_system
        .submit(AgentCommand::UserTurn(
            "请在画布中创建一个终端卡片".to_string(),
        ))
        .expect("明确创建终端指令应在模型漏调用工具时进入 CardTask 队列");

    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AgentEvent::TaskQueued { .. }))
            .count(),
        1,
        "明确创建指令必须且只能产生一个 TaskQueued"
    );
    assert!(
        agent_system.card_tool_host().commands.is_empty(),
        "TaskQueued 返回前不得提前执行画布副作用"
    );
    assert!(
        !events.iter().any(|event| matches!(
            event,
            AgentEvent::DialogueTextDelta { text } if text.contains("确认")
        )),
        "模型的多余确认文本不得在 fallback 已派发任务后继续播给用户"
    );
}

/// DP-1b：要求“别解释”不是取消创建，仍必须触发明确创建 fallback。
///
/// 数据构造：模型再次只返回确认文本；用户指令包含“别解释，直接创建终端”。
/// 执行过程：提交混合否定/执行表达。
/// 预期结果：产生 TaskQueued；“别解释”不得被粗粒度否定词误判成“别创建”。
#[test]
fn do_not_explain_create_terminal_still_queues_task() {
    let mut agent_system = system(
        ScriptedProvider::new([text_response("还需要确认吗")]),
        ScriptedProvider::default(),
        RecordingHost::default(),
    );

    let events = agent_system
        .submit(AgentCommand::UserTurn(
            "别解释，直接创建一个终端".to_string(),
        ))
        .expect("“别解释”不应阻止明确创建任务 fallback");

    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskQueued { .. })),
        "混合表达仍必须创建真实 queued 任务"
    );
}

/// DP-1c：明确创建动作中提及“设计”对象时仍必须派发，不能被说明性词表吞掉。
///
/// 数据构造：模型只返回确认文本；用户要求创建终端来实现一个已有设计方案。
/// 执行过程：提交同时含“创建终端”和“设计方案”的直接执行句。
/// 预期结果：产生真实 TaskQueued；作为执行对象的“设计方案”不得把命令降级为知识问答。
#[test]
fn create_terminal_for_existing_design_still_queues_task() {
    let mut agent_system = system(
        ScriptedProvider::new([text_response("请再说明一下设计")]),
        ScriptedProvider::default(),
        RecordingHost::default(),
    );

    let events = agent_system
        .submit(AgentCommand::UserTurn(
            "创建一个终端来实现这个设计方案".to_string(),
        ))
        .expect("明确创建动作不应因句中出现“设计”而漏掉 fallback");

    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskQueued { .. })),
        "含设计语境的明确创建动作仍必须产生 TaskQueued"
    );
}

/// DP-2b：请求“创建终端的方案”仍是讨论，不得因包含动作词和目标而执行。
///
/// 数据构造：Dialogue Provider 返回方案文本；用户索要创建方案而非要求修改画布。
/// 执行过程：提交含“创建终端的方案”的说明性请求。
/// 预期结果：保留 Dialogue 文本且没有 TaskQueued，确保 fallback 不扩大到设计讨论。
#[test]
fn request_for_terminal_creation_plan_stays_in_dialogue() {
    let mut agent_system = system(
        ScriptedProvider::new([text_response("可以先确定终端用途，再选择 shell")]),
        ScriptedProvider::default(),
        RecordingHost::default(),
    );

    let events = agent_system
        .submit(AgentCommand::UserTurn(
            "给我一个创建终端卡片的方案".to_string(),
        ))
        .expect("创建方案讨论应由 Dialogue Provider 回答");

    assert!(
        events.iter().any(|event| matches!(
            event,
            AgentEvent::DialogueTextDelta { text } if text == "可以先确定终端用途，再选择 shell"
        )),
        "方案讨论必须保留普通 Dialogue 回答"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskQueued { .. })),
        "创建方案讨论不得产生真实画布任务"
    );
}

/// DP-2：讨论如何创建卡片属于知识问答，不能被创建快速路径误判为执行命令。
///
/// 数据构造：Dialogue Provider 返回操作说明文本；Card Provider 为空。
/// 执行过程：提交“如何创建终端卡片？”这一明显的说明性问题。
/// 预期结果：保留普通 Dialogue 文本；不产生 TaskQueued；不触碰 CardToolHost。
#[test]
fn how_to_create_terminal_question_stays_in_dialogue() {
    let mut agent_system = system(
        ScriptedProvider::new([text_response("可以使用工具栏或快捷键创建终端卡片")]),
        ScriptedProvider::default(),
        RecordingHost::default(),
    );

    let events = agent_system
        .submit(AgentCommand::UserTurn("如何创建终端卡片？".to_string()))
        .expect("说明性问题应由 Dialogue Provider 正常回答");

    assert!(
        events.iter().any(|event| matches!(
            event,
            AgentEvent::DialogueTextDelta { text }
                if text == "可以使用工具栏或快捷键创建终端卡片"
        )),
        "说明性问题必须保留 Dialogue 文本回答"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskQueued { .. })),
        "“如何创建”不得被误派发为真实画布任务"
    );
    assert!(
        agent_system.card_tool_host().commands.is_empty(),
        "知识问答不得产生卡片副作用"
    );
}
