use super::*;
use crate::{CardRef, VoiceCardKind, VoicePlanningRequest};

/// OP-1: planner prompt 不应把枚举占位符写成模型可复制的 JSON 值。
///
/// 数据构造（含关键数值的推导过程）：
///   max_steps = 2（该测试只验证 risk 示例，不依赖复合语音上限）
///   risk 合法值有 3 个：low、medium、high
///   prompt 示例必须使用其中 1 个具体值，不能使用 low|medium|high 占位符
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造空画布 VoicePlanningRequest
///   2. planner_prompt 生成传给 Ollama 的完整提示词
///   3. 检查 risk 示例和非法占位符
///
/// 预期结果：
///   - 正断言：prompt 包含具体 `"risk":"low"` JSON 示例
///   - 正断言：prompt 仍说明 medium/high 是可选枚举
///   - 负断言：prompt 不应包含 `"low|medium|high"`，否则小模型可能照抄
#[test]
fn test_planner_prompt_uses_concrete_risk_example() {
    let request = VoicePlanningRequest {
        transcript: "创建 Codex".to_string(),
        cards: Vec::new(),
        focused_node_id: None,
        selected_node_ids: Vec::new(),
        max_steps: 2,
    };

    let prompt = planner_prompt(&request).expect("planner prompt 应可序列化 request");

    assert!(
        prompt.contains(r#""risk":"low""#),
        "prompt 的 JSON 示例应使用具体合法 risk=low"
    );
    assert!(
        prompt.contains("medium") && prompt.contains("high"),
        "prompt 仍应列出 medium/high 作为合法风险枚举"
    );
    assert!(
        !prompt.contains(r#""risk":"low|medium|high""#),
        "prompt 不应把 low|medium|high 写成可被模型照抄的 JSON 字符串"
    );
}

/// OP-2: planner prompt 应只注入紧凑 TARGETS，不再塞完整 canvas context 和 required hint。
///
/// 数据构造（含关键数值的推导过程）：
///   cards = 2：Agent(alias=dev, selected/focused) + Terminal
///   focused_node_id = Some(7)
///   selected_node_ids = [7]
///   max_steps = 8，但紧凑 TARGETS 不应携带 max_steps
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造含 alias、focused、selected 的 VoicePlanningRequest
///   2. planner_prompt 生成传给 Ollama 的完整提示词
///   3. 检查 prompt 中只有 TARGETS 摘要，没有旧的大段 context/hint/schema
///
/// 预期结果：
///   - 正断言：prompt 包含 TARGETS 段和压缩后的 node_id/name/kind
///   - 正断言：prompt 保留 selected/focused 用于目标路由
///   - 负断言：prompt 不应包含 CANVAS_CONTEXT、REQUIRED_HINT、ALLOWED_SCHEMA、max_steps
#[test]
fn test_planner_prompt_uses_compact_targets_without_context_or_hint() {
    let request = VoicePlanningRequest {
        transcript: "给 dev 输入 hello".to_string(),
        cards: vec![
            CardRef {
                node_id: 7,
                title: "Codex".to_string(),
                alias: Some("dev".to_string()),
                kind: VoiceCardKind::Agent {
                    profile: norma::protocol::AgentProfile::Codex,
                },
                selected: true,
                focused: true,
            },
            CardRef {
                node_id: 8,
                title: "Terminal".to_string(),
                alias: None,
                kind: VoiceCardKind::Terminal,
                selected: false,
                focused: false,
            },
        ],
        focused_node_id: Some(7),
        selected_node_ids: vec![7],
        max_steps: 8,
    };

    let prompt = planner_prompt(&request).expect("planner prompt 应可注入紧凑 TARGETS");

    assert!(
        prompt.contains("TARGETS:"),
        "prompt 应包含紧凑 TARGETS 段供模型做目标路由"
    );
    assert!(
        prompt.contains(r#""node_id":7"#)
            && prompt.contains(r#""name":"dev"#)
            && prompt.contains(r#""kind":"agent:Codex"#),
        "TARGETS 应包含 agent 的 node_id、alias 优先 name 和压缩 kind"
    );
    assert!(
        prompt.contains(r#""focused":7"#)
            && prompt.contains(r#""selected":[7]"#)
            && prompt.contains(r#""selected":true"#)
            && prompt.contains(r#""focused":true"#),
        "TARGETS 应保留 focused/selected 摘要和逐卡状态"
    );
    assert!(
        !prompt.contains("CANVAS_CONTEXT:")
            && !prompt.contains("CURRENT_CANVAS_CONTEXT:")
            && !prompt.contains("REQUIRED_HINT:")
            && !prompt.contains("REQUIRED_OUTPUT_HINT:")
            && !prompt.contains("ALLOWED_SCHEMA:")
            && !prompt.contains(r#""max_steps""#),
        "prompt 不应再塞完整 context、required hint、schema 或 max_steps"
    );
}

/// OP-3: planner prompt 应明确要求多卡片请求逐张 create，并识别 Claude Code。
///
/// 数据构造（含关键数值的推导过程）：
///   transcript = "打开codex,claude code, 再开启两个终端 然后输入hi"
///   期望 create 数 = Codex(1) + Claude(1) + Terminal(2) = 4
///   期望 send_text 数 = 1
///   Total = 4 + 1 = 5，因此 max_steps = 5
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造含复合语音的 VoicePlanningRequest
///   2. planner_prompt 生成传给 Ollama 的完整提示词
///   3. 检查 prompt 是否包含逐卡 create、Claude Code 映射和 previous_step 默认目标说明
///
/// 预期结果：
///   - 正断言：prompt 要求每张卡片各生成一条 create 命令
///   - 正断言：prompt 明确 Claude Code 映射到新的 Claude profile 卡片
///   - 正断言：prompt 保留 previous_step 输出契约和最近创建卡片路由规则
///   - 负断言：prompt 不应继续要求所有 create+send 都“exactly two commands”
#[test]
fn test_planner_prompt_guides_multi_card_create_then_send() {
    let request = VoicePlanningRequest {
        transcript: "打开codex,claude code, 再开启两个终端 然后输入hi".to_string(),
        cards: Vec::new(),
        focused_node_id: None,
        selected_node_ids: Vec::new(),
        max_steps: 5,
    };

    let prompt = planner_prompt(&request).expect("planner prompt 应可序列化复合 request");

    assert!(
        prompt.contains("Create one command for every requested card"),
        "prompt 应明确多卡片请求要逐张 create，而不是合并"
    );
    assert!(
        prompt.contains("Claude Code, 打开claude code")
            && prompt.contains("create a new agent profile Claude card"),
        "prompt 应明确 Claude Code 语义映射到新的 Claude profile 卡片"
    );
    assert!(
        prompt.contains(r#""profile":"Codex"#) && prompt.contains(r#""profile":"Claude"#),
        "输出契约应同时包含 Codex 和 Claude，避免模型漏掉 Codex"
    );
    assert!(
        prompt.contains(r#""type":"previous_step""#) && prompt.contains(r#""step":0"#),
        "输出契约应保留 previous_step 目标形状，但不依赖旧的复杂示例"
    );
    assert!(
        prompt.contains("most recently created compatible card with previous_step"),
        "prompt 应说明无显式目标的最后输入要发到最近创建的卡片"
    );
    assert!(
        !prompt.contains(r#""step":3"#),
        "prompt 不应继续携带旧的多卡片完整示例，避免提示词臃肿"
    );
    assert!(
        !prompt.contains("output exactly two commands"),
        "prompt 不应继续用旧两步规则限制复合语音"
    );
}

/// OP-4: planner prompt 应包含少量示例，覆盖带空格的中文 ASR 和多卡片 create。
///
/// 数据构造（含关键数值的推导过程）：
///   transcript = "打 开 两 个 终 端 合 一 个  codex"，来自真实日志
///   显式实体 = Terminal(2) + Codex(1)
///   Total create = 3，因此示例应展示 3 条 create 命令
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造同真实日志一致的 VoicePlanningRequest
///   2. planner_prompt 生成完整提示词
///   3. 检查 prompt 中有 EXAMPLES 段和对应三条 create 示例
///
/// 预期结果：
///   - 正断言：prompt 包含真实问题形态，提示模型空格中文仍是一个语义句
///   - 正断言：prompt 示例包含 Codex 和 Terminal create
///   - 负断言：prompt 示例不应把两个终端合并成单条命令
#[test]
fn test_planner_prompt_includes_spaced_chinese_multi_create_example() {
    let request = VoicePlanningRequest {
        transcript: "打 开 两 个 终 端 合 一 个  codex".to_string(),
        cards: Vec::new(),
        focused_node_id: None,
        selected_node_ids: Vec::new(),
        max_steps: 8,
    };

    let prompt = planner_prompt(&request).expect("planner prompt 应可序列化真实 ASR transcript");

    assert!(
        prompt.contains("EXAMPLES:") && prompt.contains("打 开 两 个 终 端 合 一 个 codex"),
        "prompt 应包含带空格中文 ASR 的 few-shot 示例"
    );
    assert!(
        prompt.contains(r#""profile":"Codex"#) && prompt.contains(r#""kind":{"kind":"terminal"}"#),
        "示例应同时展示创建 Codex 和 Terminal 的合法 JSON 形状"
    );
    assert!(
        prompt.matches(r#""kind":{"kind":"terminal"}"#).count() >= 2,
        "示例中两个终端应出现至少两条 terminal create，不能合并为一条"
    );
    assert!(
        !prompt.contains(r#""kind":[{"kind":"terminal"}"#),
        "prompt 不应展示数组 kind，否则模型可能生成非法合并命令"
    );
}

/// OP-5: JSON repair prompt 应只要求修复 planner JSON，不重新解释用户意图。
///
/// 数据构造（含关键数值的推导过程）：
///   raw response = "commands: create terminal"，不是合法 VoicePlan JSON
///   transcript = "给 codex 输入 hello"，修复时需要保留原始意图上下文
///   max_steps = 8，但 repair prompt 不应暴露为模型可改写的命令字段
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 VoicePlanningRequest
///   2. 调用 json_repair_prompt
///   3. 检查 prompt 是否包含原始 response、transcript 和输出边界
///
/// 预期结果：
///   - 正断言：repair prompt 明确 Return exactly one JSON object
///   - 正断言：repair prompt 包含 malformed response 供模型清洗
///   - 负断言：repair prompt 不应要求输出 markdown 或解释文字
#[test]
fn test_json_repair_prompt_is_bounded_to_voice_plan_json() {
    let request = VoicePlanningRequest {
        transcript: "给 codex 输入 hello".to_string(),
        cards: Vec::new(),
        focused_node_id: None,
        selected_node_ids: Vec::new(),
        max_steps: 8,
    };

    let prompt = json_repair_prompt(&request, "commands: create terminal")
        .expect("repair prompt 应可序列化 malformed response");

    assert!(
        prompt.contains("Return exactly one JSON object") && prompt.contains("VoicePlan JSON"),
        "repair prompt 应明确只返回 VoicePlan JSON 对象"
    );
    assert!(
        prompt.contains("commands: create terminal") && prompt.contains("给 codex 输入 hello"),
        "repair prompt 应同时包含坏输出和原始 transcript"
    );
    assert!(
        !prompt.contains("```") && !prompt.contains("markdown"),
        "repair prompt 不应鼓励 markdown 包装"
    );
}
