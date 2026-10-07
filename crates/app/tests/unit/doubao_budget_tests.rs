use super::plan::chat;
use serde_json::json;

/// 开启项目语音后的第一句“你好”必须通过生产请求预算，不能触发会话断开。
///
/// 数据构造（含关键数值的推导过程）：
///   画布卡片 = 0，会话尾部 = 0，历史只有 1 条 user“你好”（2 字 × 3 B = 6 B）。
///   system = 共享 Norma 规则 + 空项目环境 + 默认人设；tools = 完整生产工具定义。
///   原预算 = 16,384 tokens × 2 B - 1,024 tokens × 2 B - 8,192 B = 22,528 B。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 按项目语音生产入口组装固定规则、当前画布与人设。
///   2. 用唯一的本轮 user 消息调用 bounded_request，启用项目工具。
///   3. 检查请求可提交、问候保留且项目派发工具仍可用。
///
/// 预期结果：
///   - 正断言：请求构造成功，包含“你好”和 assign_agent_task。
///   - 负断言：首句不得产生 context_budget 错误，不能删除当前 user。
#[test]
fn voice_first_project_utterance_fits_request_budget() {
    let canvas = agent::CurrentCanvasContext {
        project_name: "voice-budget-workspace".into(),
        workspace_root: "/tmp/voice-budget-workspace".into(),
        total_cards: 0,
        cards: Vec::new(),
        conversation_tails: Vec::new(),
    };
    let mut prompt = agent::norma_system_prompt_base(&canvas, &[]);
    prompt.push_str(&agent::NormaPersona::default().prompt());
    let mut history = vec![json!({"role":"user","content":"你好"})];
    let unbounded_bytes = chat::request_body(&prompt, &history, true)
        .to_string()
        .len();
    let result = chat::bounded_request(&prompt, &mut history, true, false);
    assert!(
        result.is_ok(),
        "合法首句不应导致语音断开；生产请求 {unbounded_bytes} B，错误：{result:?}"
    );
    let body = result.expect("首句请求应成功");
    assert_eq!(body["messages"][1]["content"], "你好", "本轮问候必须保留");
    assert!(
        body["tools"]
            .as_array()
            .expect("项目工具应存在")
            .iter()
            .any(|tool| tool["function"]["name"] == "assign_agent_task"),
        "修复预算不能移除项目派发能力"
    );
    assert_eq!(history.len(), 1, "唯一的当前 user 不得被裁剪");
}

/// 项目已有合法会话尾部时，开启语音后的首句不能因系统上下文过大而断开。
///
/// 数据构造（含关键数值的推导过程）：
///   4 条 Agent 尾部 × 每条 1,200 个汉字 × 3 B = 14,400 B 正文。
///   4,800 字 < 全局 8,000 字上限；每条 1,200 字恰好达到生产单条上限。
///   历史 = 1 条 user“你好”，不是长请求，也没有可裁剪的旧语音回合。
///   原请求预算 = 16,384 × 2 - 1,024 × 2 - 8,192 = 22,528 B。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 为当前项目构造四个已完成 Agent 的合法会话尾部。
///   2. 通过共享环境渲染生成完整 system，追加默认人设。
///   3. 走生产 bounded_request，检查当前问候与合法上下文是否可提交。
///
/// 预期结果：
///   - 正断言：请求成功，当前 user 和第一条、第四条 Agent 尾部保留。
///   - 负断言：不得返回会导致 run_session 退出的 context_budget 错误。
#[test]
fn voice_first_utterance_with_canvas_tails_fits_request_budget() {
    let tails = (1..=4)
        .map(|id| agent::AgentConversationTail {
            definition_id: id,
            node_id: id as u64,
            session_id: format!("session-{id}"),
            agent_name: format!("Agent-{id}"),
            profile: norma::protocol::AgentProfile::Codex,
            task_state: agent::AgentConversationTaskState::Done,
            last_user_message: None,
            last_agent_message: Some("文".repeat(agent::MAX_TAIL_MESSAGE_CHARS)),
            updated_at_unix: id,
            availability: agent::AgentConversationAvailability::Available,
            user_message_truncated: false,
            agent_message_truncated: false,
        })
        .collect();
    let canvas = agent::CurrentCanvasContext {
        project_name: "voice-budget-workspace".into(),
        workspace_root: "/tmp/voice-budget-workspace".into(),
        total_cards: 4,
        cards: Vec::new(),
        conversation_tails: tails,
    };
    let mut prompt = agent::norma_system_prompt_base(&canvas, &[]);
    prompt.push_str(&agent::NormaPersona::default().prompt());
    let mut history = vec![json!({"role":"user","content":"你好"})];
    let unbounded_bytes = chat::request_body(&prompt, &history, true)
        .to_string()
        .len();
    let result = chat::bounded_request(&prompt, &mut history, true, false);
    assert!(
        result.is_ok(),
        "合法画布尾部不应导致首句断开；请求 {unbounded_bytes} B，错误：{result:?}"
    );
    let body = result.expect("合法上下文应可容纳");
    let system = body["messages"][0]["content"]
        .as_str()
        .expect("system 应存在");
    assert!(system.contains("Agent-1"), "第一条已有会话必须保留");
    assert!(system.contains("Agent-4"), "第四条已有会话必须保留");
    assert_eq!(body["messages"][1]["content"], "你好", "首句问候必须保留");
    assert!(
        !system.contains("messages=omitted_due_to_budget"),
        "4,800 字的合法尾部不得被当作超出 8,000 字上限"
    );
}

/// 当前回合确实超出模型输入上限时必须明确拒绝，不能裁掉当前用户或工具结果。
///
/// 数据构造（含关键数值的推导过程）：
///   输入预算 = 224,000 × 2 - 8,192 = 439,808 B。
///   当前 user = budget + 1 个 ASCII x = 439,809 B，正文已超限。
///   当前 assistant(call-current) + tool(call-current) = 配对的 2 条消息。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造唯一当前回合和已提交的工具配对。
///   2. 调用 bounded_request；无可裁剪的旧 user 回合。
///   3. 检查明确的预算错误和未变动的当前历史。
///
/// 预期结果：
///   - 正断言：返回 context_budget，当前用户、工具调用和回执全部保留。
///   - 负断言：不得放行超限请求，不得删掉当前回合来伪造可提交请求。
#[test]
fn voice_request_rejects_oversized_current_turn_without_pruning_it() {
    // ASCII x 占 1 B：budget + 1 使正文严格超限，JSON/system/schema 只会增加长度。
    let current = "x".repeat(chat::REQUEST_INPUT_BYTES + 1);
    let mut history = vec![
        json!({"role":"user","content":current}),
        json!({"role":"assistant","tool_calls":[{"id":"call-current","function":{"name":"memory_read","arguments":"{}"}}]}),
        json!({"role":"tool","tool_call_id":"call-current","content":"本轮结果"}),
    ];
    let result = chat::bounded_request("当前项目", &mut history, true, false);
    assert!(result.is_err(), "真正超限的当前回合不得提交给模型");
    assert_eq!(
        result.expect_err("应有明确预算错误"),
        ui::i18n::LocalizedText::new("doubao.context_budget"),
        "必须报告预算错误而不是静默裁剪当前消息"
    );
    assert_eq!(history[0]["content"], current, "当前用户正文必须完整保留");
    assert_eq!(
        history[1]["tool_calls"][0]["id"], "call-current",
        "当前工具调用不能被当作旧回合删除"
    );
    assert_eq!(
        history[2]["tool_call_id"], "call-current",
        "当前工具结果不能失去对应调用"
    );
    assert_eq!(history[2]["content"], "本轮结果", "当前工具回执不能丢失");
}
