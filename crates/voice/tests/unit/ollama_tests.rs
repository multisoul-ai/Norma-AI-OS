use super::*;
use crate::{
    compile_voice_plan, ExecutableCommand, PlannedCardKind, PlannedCommand, VoicePlanDecision,
    VoicePlanningRequest,
};
use serde_json::Value;

/// O-1: Ollama 返回 fenced JSON 时 provider 应提取内部 VoicePlan。
///
/// 数据构造（含关键数值的推导过程）：
///   commands = 1，低于 VoicePlanningRequest v1 上限 2
///   confidence = 0.88，高于 MIN_EXECUTE_CONFIDENCE(0.65)
///   raw response 在 JSON 前后各包一层 markdown fence
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 qwen 常见的 ```json fenced response
///   2. parse_voice_plan_response 去掉非 JSON 包裹
///   3. serde_json 反序列化为 VoicePlan
///
/// 预期结果：
///   - 正断言：解析后只有 1 条 create terminal 命令
///   - 正断言：risk 保持 low，说明解析的是内部 JSON
///   - 负断言：commands 不应为空，否则 provider 会把可执行计划丢成空计划
#[test]
fn test_parse_voice_plan_response_accepts_fenced_json() {
    let raw = r#"```json
{
  "commands": [
    { "type": "create", "kind": { "kind": "terminal" } }
  ],
  "confidence": 0.88,
  "risk": "low",
  "requires_confirmation": false,
  "reason": "create terminal"
}
```"#;

    let plan = parse_voice_plan_response(raw).expect("fenced JSON 应被提取并解析为 VoicePlan");

    assert_eq!(
        plan.commands.len(),
        1,
        "fenced JSON 内部只有一条命令，解析后应保持为 1 条"
    );
    assert_eq!(
        plan.commands[0],
        PlannedCommand::Create {
            kind: PlannedCardKind::Terminal,
        },
        "内部 create terminal 命令应被保留，而不是被 markdown fence 破坏"
    );
    assert_eq!(
        plan.risk,
        crate::RiskLevel::Low,
        "risk 应来自内部 JSON 的 low，而不是解析失败后走默认值"
    );
    assert_ne!(
        plan.commands.len(),
        0,
        "provider 不应把 fenced JSON 误判为空计划"
    );
}

/// O-2: Ollama generate 请求必须启用 JSON mode，减少小模型输出自然语言包装。
///
/// 数据构造（含关键数值的推导过程）：
///   model = qwen3.5:0.8b，是当前默认本地小模型
///   stream = false，期望一次性返回完整 response 字段
///   format = json，要求 Ollama 对输出做 JSON mode 约束
///   temperature = 0.0，减少相同 transcript 的计划抖动
///   think = false，关闭 qwen3.5 的 thinking trace，避免占用响应预算
///
/// 执行过程（逐步说明系统如何处理）：
///   1. build_generate_body 生成 /api/generate 请求体
///   2. 将请求体解析成 serde_json::Value
///   3. 检查 model/prompt/stream/format/think/options 六个关键字段
///
/// 预期结果：
///   - 正断言：format 字段等于 json
///   - 正断言：think 字段等于 false
///   - 正断言：temperature 字段等于 0.0
///   - 正断言：stream=false，provider 能按完整 body 解析
///   - 负断言：请求体不应缺失 format，否则 qwen 易输出 markdown fence
#[test]
fn test_generate_body_requests_ollama_json_mode() {
    let body = build_generate_body("qwen3.5:0.8b", "planner prompt")
        .expect("Ollama 请求体应可序列化为 JSON");
    let value: Value = serde_json::from_slice(&body).expect("请求体应是合法 JSON");

    assert_eq!(
        value["model"], "qwen3.5:0.8b",
        "请求体应携带配置的 Ollama model"
    );
    assert_eq!(
        value["prompt"], "planner prompt",
        "请求体应携带 voice planner prompt"
    );
    assert_eq!(
        value["stream"], false,
        "stream=false 才能让 provider 读取单个完整 response body"
    );
    assert_eq!(
        value["format"], "json",
        "format=json 应启用 Ollama JSON mode"
    );
    assert_eq!(
        value["think"], false,
        "think=false 应显式关闭 qwen3.5 thinking mode"
    );
    assert_eq!(
        value["options"]["temperature"], 0.0,
        "temperature=0.0 应降低 live planner 的输出抖动"
    );
    assert!(
        value.get("format").is_some(),
        "请求体缺失 format 会让小模型更容易输出 markdown 包装"
    );
}

/// O-3: provider 应能解析 Ollama 的 chunked HTTP response body。
///
/// 数据构造（含关键数值的推导过程）：
///   chunk body = 1 个 JSON chunk + 结束 chunk
///   JSON chunk 长度由 bytes.len() 转为十六进制，模拟 Transfer-Encoding: chunked
///   response.response 内部是一个合法 VoicePlan JSON 字符串
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造带 Transfer-Encoding: chunked header 的 HTTP raw response
///   2. parse_ollama_http_response 解码 chunk size 与 chunk body
///   3. 返回 OllamaGenerateResponse，再由 provider 解析 response 字段
///
/// 预期结果：
///   - 正断言：response 字段等于内部 VoicePlan JSON
///   - 正断言：内部 VoicePlan 可继续解析成 create terminal
///   - 负断言：解析结果不应以 chunk size 开头
#[test]
fn test_parse_ollama_http_response_accepts_chunked_body() {
    let plan_json = r#"{"commands":[{"type":"create","kind":{"kind":"terminal"}}],"confidence":0.88,"risk":"low","requires_confirmation":false,"reason":"create terminal"}"#;
    let generate_json = serde_json::json!({ "response": plan_json }).to_string();
    let raw = format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:X}\r\n{}\r\n0\r\n\r\n",
        generate_json.len(),
        generate_json
    );

    let response = parse_ollama_http_response(&raw).expect("chunked body 应解码为 Ollama response");
    let plan =
        parse_voice_plan_response(&response.response).expect("response 字段应继续解析为 VoicePlan");

    assert_eq!(
        response.response, plan_json,
        "chunked response 解码后应只保留 JSON body，不含 chunk metadata"
    );
    assert_eq!(
        plan.commands[0],
        PlannedCommand::Create {
            kind: PlannedCardKind::Terminal,
        },
        "解码后的 response 字段应可解析为 create terminal"
    );
    assert!(
        !response
            .response
            .starts_with(&format!("{:X}", generate_json.len())),
        "provider 不应把 chunk size 当成 JSON body 的一部分"
    );
}

/// O-7: 默认 Ollama 模型应是已验证的小型中文模型。
///
/// 数据构造（含关键数值的推导过程）：
///   qwen3.5:0.8b 是当前默认本地小模型
///   provider 仍允许 MULTISOUL_OLLAMA_MODEL 覆盖，此测试只锁默认常量
///   默认值非空，app 才能在未设置 env 时尝试 planner
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 OllamaVoicePlanner::default_model()
///   2. 构造 planner 并读取 planner.model()
///   3. 比较二者与预期模型名
///
/// 预期结果：
///   - 正断言：默认模型名是 qwen3.5:0.8b
///   - 正断言：planner 会保存传入的默认模型名
///   - 负断言：默认模型名不能为空，否则 app 无法开箱使用 planner
#[test]
fn test_default_ollama_model_is_small_chinese_qwen() {
    let model = OllamaVoicePlanner::default_model();
    let planner = OllamaVoicePlanner::new("http://127.0.0.1:11434", model);

    assert_eq!(
        model, "qwen3.5:0.8b",
        "默认模型应切到当前指定的 qwen3.5 0.8B 模型"
    );
    assert_eq!(
        planner.model(),
        "qwen3.5:0.8b",
        "planner 应保存默认模型名用于请求和审计"
    );
    assert!(
        !model.is_empty(),
        "默认模型名不能为空，否则未设置 env 时 planner 不可用"
    );
}

/// O-8: MULTISOUL_VOICE_TRACE 只接受明确 truthy 值，避免误开完整 prompt 日志。
///
/// 数据构造（含关键数值的推导过程）：
///   enabled values = ["1", "true", "yes", "on"]，共 4 个显式开启值
///   disabled values = None / "0" / "false" / ""，共 4 个关闭或缺省值
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将每个 enabled value 传入 voice_trace_enabled_from_value
///   2. 将每个 disabled value 传入 voice_trace_enabled_from_value
///   3. 比较返回布尔值是否符合日志开关语义
///
/// 预期结果：
///   - 正断言：明确 truthy 值会开启完整 prompt/response debug 日志
///   - 负断言：缺省、空值和 false-like 值不会误开完整 prompt 日志
#[test]
fn test_voice_trace_env_parser_accepts_only_truthy_values() {
    assert!(
        voice_trace_enabled_from_value(Some("1".to_string())),
        "MULTISOUL_VOICE_TRACE=1 应开启完整 voice planner 日志"
    );
    assert!(
        voice_trace_enabled_from_value(Some(" true ".to_string())),
        "MULTISOUL_VOICE_TRACE=true 前后空白应被 trim 后识别为开启"
    );
    assert!(
        voice_trace_enabled_from_value(Some("YES".to_string())),
        "MULTISOUL_VOICE_TRACE=YES 应大小写不敏感地识别为开启"
    );
    assert!(
        voice_trace_enabled_from_value(Some("on".to_string())),
        "MULTISOUL_VOICE_TRACE=on 应开启完整 voice planner 日志"
    );
    assert!(
        !voice_trace_enabled_from_value(None),
        "未设置 MULTISOUL_VOICE_TRACE 时不应误开完整 prompt 日志"
    );
    assert!(
        !voice_trace_enabled_from_value(Some("0".to_string())),
        "MULTISOUL_VOICE_TRACE=0 不应开启完整 prompt 日志"
    );
    assert!(
        !voice_trace_enabled_from_value(Some("false".to_string())),
        "MULTISOUL_VOICE_TRACE=false 不应开启完整 prompt 日志"
    );
    assert!(
        !voice_trace_enabled_from_value(Some("".to_string())),
        "MULTISOUL_VOICE_TRACE 为空字符串不应开启完整 prompt 日志"
    );
}

/// O-12: 本地 Ollama live planner 应能把中文 transcript 转成 create -> send 计划。
///
/// 数据构造（含关键数值的推导过程）：
///   model 默认 qwen3.5:0.8b，可用 MULTISOUL_VOICE_LIVE_OLLAMA_MODEL 覆盖
///   transcript 明确包含 Codex + 发送 hello + 不提交
///   max_steps = 2，期望输出 create agent 和 send_text previous_step 两步
///
/// 执行过程（逐步说明系统如何处理）：
///   1. OllamaVoicePlanner 通过 localhost /api/generate 调用本地模型
///   2. provider 解析模型 response 为 VoicePlan
///   3. compile_voice_plan 校验 schema、步数、置信度和 previous_step 引用
///
/// 预期结果：
///   - 正断言：编译结果是 Execute，说明 planner 输出可执行
///   - 正断言：第一步创建 Codex Agent，第二步发送 hello 到 previous_step=0
///   - 负断言：第二步不应 submit，否则“不要提交”被忽略
#[test]
#[ignore = "requires a running local Ollama service and pulled model"]
fn test_live_ollama_planner_creates_agent_then_send() {
    let model = std::env::var("MULTISOUL_VOICE_LIVE_OLLAMA_MODEL")
        .unwrap_or_else(|_| "qwen3.5:0.8b".to_string());
    let request = VoicePlanningRequest {
        transcript: "创建一个 Codex 卡片，并向它发送 hello，不要提交".to_string(),
        cards: Vec::new(),
        focused_node_id: None,
        selected_node_ids: Vec::new(),
        max_steps: 2,
    };
    let planner = OllamaVoicePlanner::new("http://127.0.0.1:11434", model);

    let plan = planner
        .plan(&request)
        .expect("本地 Ollama 应返回合法 VoicePlan");
    eprintln!("live ollama plan: {plan:?}");
    let decision = compile_voice_plan(&request, plan).expect("live VoicePlan 应通过编译校验");
    let VoicePlanDecision::Execute(compiled) = decision else {
        panic!("明确 create+send 的中文 transcript 不应降级为 transcript-only");
    };

    assert_eq!(
        compiled.commands.len(),
        2,
        "live planner 应输出 create -> send 两步计划"
    );
    assert_eq!(
        compiled.commands[0],
        ExecutableCommand::Card(norma::protocol::CardCommand::Create {
            kind: norma::protocol::CardKind::Agent {
                profile: norma::protocol::AgentProfile::Codex,
            },
            placement: norma::protocol::Placement::VisibleArea,
        }),
        "第一步应创建 Codex Agent 卡片"
    );
    assert_eq!(
        compiled.commands[1],
        ExecutableCommand::SendTextToPrevious {
            previous_step: 0,
            text: "hello".to_string(),
            submit: false,
        },
        "第二步应把 hello 发送给第 0 步创建的卡片且不 submit"
    );
    assert_ne!(
        compiled.commands[1],
        ExecutableCommand::SendTextToPrevious {
            previous_step: 0,
            text: "hello".to_string(),
            submit: true,
        },
        "用户明确不要提交时，live planner 不应生成 submit=true"
    );
}
