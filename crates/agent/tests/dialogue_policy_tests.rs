pub mod support;

use agent::{AgentCommand, AgentEvent, AgentRosterEntry, AgentTrackingMode};
use norma::protocol::AgentProfile;
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

/// DP-2c：说明性问题即使被 Provider 误写成非法 start，也必须由本地策略阻止副作用。
///
/// 数据构造（含关键数值的推导过程）：
///   user questions       = 1：“如何创建终端卡片？”
///   provider tool calls  = 1：错误的 card_task start
///   invalid assignments = 1：card_operation 错带“操作执行”assignment
///   expected tasks       = 0：说明性问题不得创建 CardTask
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 重放 MiniMax 对说明性问题返回的错误 card_task 参数
///   2. 本地 Dialogue 路由先把该轮标记为只允许对话
///   3. 在解析错误参数或创建任务之前丢弃该工具调用
///   4. 保留 Provider 同一响应中的说明文本
///
/// 预期结果：
///   - 正断言：用户仍收到说明性 DialogueTextDelta
///   - 负断言：不得返回参数错误、产生 TaskQueued 或触碰 CardToolHost
#[test]
fn explanatory_question_rejects_provider_false_start_before_parsing_or_side_effects() {
    let dialogue = ScriptedProvider::new([agent::ProviderResponse {
        text: "可以从画布工具栏创建终端卡片".to_string(),
        tool_calls: vec![support::tool(
            "false-start-1",
            agent::CARD_TASK_TOOL,
            r#"{
                "action":"start",
                "task_kind":"card_operation",
                "assignments":[{"name":"操作执行","task":"创建一个终端卡片"}]
            }"#,
        )],
    }]);
    let mut agent_system = system(
        dialogue,
        ScriptedProvider::default(),
        RecordingHost::default(),
    );

    let result = agent_system.submit(AgentCommand::UserTurn("如何创建终端卡片？".to_string()));

    assert!(
        result.is_ok(),
        "说明性问题中的 Provider 错误工具调用必须被本地策略拦截，实际：{result:?}"
    );
    let events = result.expect("前一断言已确认说明性问题不会返回工具参数错误");
    assert!(
        events.iter().any(|event| matches!(
            event,
            AgentEvent::DialogueTextDelta { text }
                if text == "可以从画布工具栏创建终端卡片"
        )),
        "拦截错误 start 后必须保留同一响应中的说明文本"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskQueued { .. })),
        "说明性问题不得因 Provider 误调用而产生 TaskQueued"
    );
    assert!(
        agent_system.card_tool_host().commands.is_empty(),
        "说明性问题不得触碰 CardToolHost 或产生画布副作用"
    );
}

/// DP-2d：创建多个 Agent 后使用单数指代时，必须先澄清接收者而不是部分执行。
///
/// 数据构造（含关键数值的推导过程）：
///   agent candidates    = 2：Codex Alan + Claude Bob
///   singular references = 1：“让它分析 README”
///   provider starts     = 1：错误地只创建 Alan
///   expected tasks      = 0：候选数 2 > 唯一候选 1，不能猜测
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用户在同一句中要求创建两个 Agent，后续只使用单数“它”
///   2. Provider 错误选择 Alan 并返回 card_task start
///   3. 本地策略识别多候选单数指代，在解析和入队前拒绝工具调用
///   4. 保留 Provider 的最小澄清文本
///
/// 预期结果：
///   - 正断言：用户收到“请指定 Alan 或 Bob”的澄清文本
///   - 负断言：不得只创建 Alan、产生 TaskQueued 或调用 CardToolHost
#[test]
fn ambiguous_multi_agent_pronoun_rejects_provider_partial_start() {
    let dialogue = ScriptedProvider::new([agent::ProviderResponse {
        text: "请指定由 Alan 还是 Bob 分析 README".to_string(),
        tool_calls: vec![support::tool(
            "ambiguous-start-1",
            agent::CARD_TASK_TOOL,
            r#"{
                "action":"start",
                "task_kind":"card_operation",
                "intent":"创建 Codex Agent Alan"
            }"#,
        )],
    }]);
    let mut agent_system = system(
        dialogue,
        ScriptedProvider::default(),
        RecordingHost::default(),
    );

    let events = agent_system
        .submit(AgentCommand::UserTurn(
            "创建 Codex Agent Alan 和 Claude Agent Bob，让它分析 README".to_string(),
        ))
        .expect("多候选单数指代应进入澄清，而不是返回工具参数错误");

    assert!(
        events.iter().any(|event| matches!(
            event,
            AgentEvent::DialogueTextDelta { text }
                if text == "请指定由 Alan 还是 Bob 分析 README"
        )),
        "多候选单数指代必须保留最小澄清文本"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskQueued { .. })),
        "多候选单数指代不得部分创建 Alan 或产生 TaskQueued"
    );
    assert!(
        agent_system.card_tool_host().commands.is_empty(),
        "澄清完成前不得调用 CardToolHost 或产生部分副作用"
    );
}

fn alan_roster() -> Vec<AgentRosterEntry> {
    vec![AgentRosterEntry {
        definition_id: 1,
        name: "Alan".to_string(),
        speech_alias: Some("艾伦".to_string()),
        profile: AgentProfile::Codex,
        enabled: true,
        active_node_id: Some(101),
        tracking: AgentTrackingMode::Managed,
        busy: false,
    }]
}

/// DP-3：模型漏调工具时，明确双语呼叫应 fallback；普通提及同名 Agent 不得误派发。
///
/// 数据构造（含关键数值的推导过程）：
///   roster = 1 个 Alan，中文语音别名 = 艾伦
///   explicit = “艾伦，开发一个贪吃蛇”，含 1 个名称开头呼叫与非空任务正文
///   mention = “总结 Alan 昨天的工作”，名称位于普通宾语位置
///   两个 Dialogue Provider 都返回纯文本、工具调用数 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向第一个 AgentSystem 注入 roster 并提交 explicit → 路由策略补一次 card_task
///   2. 向第二个 AgentSystem 注入相同 roster 并提交 mention → 保持 ModelOnly
///   3. 分别观察公开 AgentEvent
///
/// 预期结果：
///   - 正断言：明确“艾伦，”呼叫恰好产生 1 个 TaskQueued
///   - 正断言：普通提及保留 DialogueTextDelta
///   - 负断言：普通提及不能产生 TaskQueued，明确呼叫不能播报模型的漏调用文本
#[test]
fn explicit_agent_address_falls_back_but_ordinary_mention_stays_in_dialogue() {
    let mut explicit_system = system(
        ScriptedProvider::new([text_response("你可以自己打开 Codex")]),
        ScriptedProvider::default(),
        RecordingHost::default(),
    );
    explicit_system
        .set_agent_roster(alan_roster())
        .expect("Alan/艾伦 名册应可用于确定性路由");
    let explicit_events = explicit_system
        .submit(AgentCommand::UserTurn("艾伦，开发一个贪吃蛇".to_string()))
        .expect("明确呼叫应在模型漏调工具时进入 CardTask");

    let mut mention_system = system(
        ScriptedProvider::new([text_response("我可以帮你整理 Alan 的工作记录")]),
        ScriptedProvider::default(),
        RecordingHost::default(),
    );
    mention_system
        .set_agent_roster(alan_roster())
        .expect("相同名册应可用于普通提及保护");
    let mention_events = mention_system
        .submit(AgentCommand::UserTurn("总结 Alan 昨天的工作".to_string()))
        .expect("普通提及应作为 Dialogue 处理");

    assert_eq!(
        explicit_events
            .iter()
            .filter(|event| matches!(event, AgentEvent::TaskQueued { .. }))
            .count(),
        1,
        "明确中文语音别名呼叫必须且只能 fallback 一次"
    );
    assert!(
        !explicit_events.iter().any(|event| matches!(
            event,
            AgentEvent::DialogueTextDelta { text } if text.contains("自己打开")
        )),
        "明确呼叫 fallback 后不得继续播报模型漏调用工具的建议文本"
    );
    assert!(
        mention_events.iter().any(|event| matches!(
            event,
            AgentEvent::DialogueTextDelta { text } if text.contains("整理 Alan")
        )),
        "普通提及必须保留 Dialogue Provider 的正常回答"
    );
    assert!(
        !mention_events
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskQueued { .. })),
        "普通宾语位置提及 Alan 不得产生执行任务"
    );
}

/// DP-4：用户完整明确目标后，模型的多余追问必须被确定性 fallback 屏蔽。
///
/// 数据构造（含关键数值的推导过程）：
///   roster = 2 个 Agent：Alan(id=1) + Bob(id=2)；
///   dialogue responses = 2 条：首次最小澄清 1 条 + 第二次多余追问 1 条；
///   first turn = “让它分析 README”，目标候选数 = 2，不能唯一寻址；
///   second turn = “让 Alan 分析 README”，精确目标数 = 1，动作数 = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 提交第一轮多候选指代 → 保留 Provider 的一次澄清，不产生 CardTask；
///   2. 用户用完整指令明确 Alan → 确定性路由要求 CardTask；
///   3. Provider 仍返回追问文本 → fallback 丢弃追问并补入唯一 TaskQueued。
///
/// 预期结果：
///   - 正断言：第一轮显示一次包含 Alan/Bob 的最小澄清；
///   - 正断言：第二轮完整明确目标后恰好产生一个 TaskQueued；
///   - 负断言：第一轮不得派发，第二轮不得继续显示确认或补充细节文本。
#[test]
fn clarified_agent_target_dispatches_without_repeated_confirmation() {
    let dialogue = ScriptedProvider::new([
        text_response("请确认让 Alan 还是 Bob 分析 README"),
        text_response("请再补充分析范围和验收标准"),
    ]);
    let mut agent_system = system(
        dialogue,
        ScriptedProvider::default(),
        RecordingHost::default(),
    );
    agent_system
        .set_agent_roster(vec![
            AgentRosterEntry {
                definition_id: 1,
                name: "Alan".to_string(),
                speech_alias: Some("艾伦".to_string()),
                profile: AgentProfile::Codex,
                enabled: true,
                active_node_id: Some(101),
                tracking: AgentTrackingMode::Managed,
                busy: false,
            },
            AgentRosterEntry {
                definition_id: 2,
                name: "Bob".to_string(),
                speech_alias: None,
                profile: AgentProfile::Claude,
                enabled: true,
                active_node_id: Some(202),
                tracking: AgentTrackingMode::Managed,
                busy: false,
            },
        ])
        .expect("Alan 与 Bob 的唯一名册应可用于多候选澄清测试");

    let ambiguous_events = agent_system
        .submit(AgentCommand::UserTurn("让它分析 README".to_string()))
        .expect("多候选指代应由 Dialogue 返回最小澄清");
    let clarified_events = agent_system
        .submit(AgentCommand::UserTurn("让 Alan 分析 README".to_string()))
        .expect("用户明确 Alan 后必须直接进入 CardTask 队列");

    assert!(
        ambiguous_events.iter().any(|event| matches!(
            event,
            AgentEvent::DialogueTextDelta { text }
                if text == "请确认让 Alan 还是 Bob 分析 README"
        )),
        "多候选指代时应保留一次同时列出 Alan 与 Bob 的最小澄清"
    );
    assert!(
        !ambiguous_events
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskQueued { .. })),
        "目标仍有 Alan/Bob 两个候选时不得猜测派发"
    );
    assert_eq!(
        clarified_events
            .iter()
            .filter(|event| matches!(event, AgentEvent::TaskQueued { .. }))
            .count(),
        1,
        "用户明确 Alan 与分析动作后必须且只能产生一个 TaskQueued"
    );
    assert!(
        !clarified_events.iter().any(|event| matches!(
            event,
            AgentEvent::DialogueTextDelta { text }
                if text.contains("补充") || text.contains("确认")
        )),
        "用户解除目标歧义后必须屏蔽模型的第二次确认或补充细节请求"
    );
}
