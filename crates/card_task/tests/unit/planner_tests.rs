use card_task::{compile_voice_plan_for_card_task, parse_voice_plan_json, CardTaskPlannerError};
use norma::protocol::AgentProfile;
use voice::{
    CardRef, PlannedCardKind, PlannedCommand, PlannedTarget, RiskLevel, VoiceCardKind, VoicePlan,
    VoicePlanningRequest,
};

fn planning_request(max_steps: usize) -> VoicePlanningRequest {
    VoicePlanningRequest {
        transcript: "创建一个 Codex 卡片".to_string(),
        cards: vec![CardRef {
            node_id: 7,
            title: "Alan".to_string(),
            alias: Some("alan".to_string()),
            kind: VoiceCardKind::Agent {
                profile: AgentProfile::Codex,
            },
            selected: true,
            focused: true,
        }],
        focused_node_id: Some(7),
        selected_node_ids: vec![7],
        max_steps,
    }
}

/// CP-1：合法 planner JSON 应解析成 VoicePlan，并保留命令与风险字段。
///
/// 数据构造（含关键数值的推导过程）：
///   commands = 1（create Codex Agent）
///   confidence = 0.90 > MIN_EXECUTE_CONFIDENCE(0.65)
///   risk = low
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造一段合法 planner JSON
///   2. 调用 parse_voice_plan_json(json)
///   3. 读取返回 VoicePlan 的 commands / confidence / risk
///
/// 预期结果：
///   - 正断言：命令数为 1，且首条是 create agent
///   - 正断言：confidence 与 risk 被完整保留
///   - 负断言：解析结果不应被误判成 JSON error
#[test]
fn test_parse_voice_plan_json_preserves_valid_plan_fields() {
    let planner_json = r#"{
        "commands":[{"type":"create","kind":{"kind":"agent","profile":"Codex"}}],
        "confidence":0.9,
        "risk":"low",
        "requires_confirmation":false
    }"#;

    let plan = parse_voice_plan_json(planner_json).expect("合法 planner JSON 应解析成 VoicePlan");

    assert_eq!(
        plan.commands.len(),
        1,
        "合法 planner JSON 应保留一条 create 命令"
    );
    assert_eq!(
        plan.commands[0],
        PlannedCommand::Create {
            kind: PlannedCardKind::Agent {
                profile: AgentProfile::Codex,
            },
        },
        "首条 planner 命令应解析成 create codex agent"
    );
    assert_eq!(
        plan.confidence, 0.9,
        "planner confidence 应原样保留，后续 compile 才能正确判定 execute/transcript-only"
    );
    assert_eq!(
        plan.risk,
        RiskLevel::Low,
        "planner risk 应原样保留，供 callback 和日志复用"
    );
    assert_ne!(
        parse_voice_plan_json(planner_json),
        Err(CardTaskPlannerError::Json {
            message: "malformed".to_string(),
        }),
        "合法 planner JSON 不应落入 JSON error 分支"
    );
}

/// CP-2：畸形 planner JSON 应返回 CardTaskPlannerError::Json。
///
/// 数据构造（含关键数值的推导过程）：
///   malformed json = "{not valid"
///   byte length = 10；缺少 key/value 与结尾大括号
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用 parse_voice_plan_json("{not valid")
///   2. 读取错误枚举类型
///
/// 预期结果：
///   - 正断言：返回 Json error
///   - 负断言：不应误判成 Provider 或 Compile error
#[test]
fn test_parse_voice_plan_json_reports_json_error_for_malformed_payload() {
    let error = parse_voice_plan_json("{not valid").expect_err("畸形 planner JSON 必须被拒绝");

    assert!(
        matches!(error, CardTaskPlannerError::Json { .. }),
        "畸形 planner JSON 必须映射成 CardTaskPlannerError::Json"
    );
    assert!(
        !matches!(error, CardTaskPlannerError::Provider { .. }),
        "畸形 planner JSON 不应伪装成 provider 错误"
    );
    assert!(
        !matches!(error, CardTaskPlannerError::Compile { .. }),
        "畸形 planner JSON 不应伪装成 compile rejection"
    );
}

/// CP-3：可执行 VoicePlan 应编译成 CompiledVoicePlan。
///
/// 数据构造（含关键数值的推导过程）：
///   request.max_steps = 4
///   commands = 2（create terminal + focus focused）
///   confidence = 0.92 > 0.65，因此应走 Execute 分支
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造两步 VoicePlan
///   2. 调用 compile_voice_plan_for_card_task(request, plan)
///   3. 读取 compiled.commands
///
/// 预期结果：
///   - 正断言：返回 2 条 compiled commands
///   - 正断言：第一步为 create terminal，第二步为 focus existing card
///   - 负断言：合法可执行 plan 不应被降级成 TranscriptOnly rejection
#[test]
fn test_compile_voice_plan_for_card_task_returns_executable_commands() {
    let request = planning_request(4);
    let plan = VoicePlan {
        commands: vec![
            PlannedCommand::Create {
                kind: PlannedCardKind::Terminal,
            },
            PlannedCommand::Focus {
                target: PlannedTarget::Focused,
            },
        ],
        confidence: 0.92,
        risk: RiskLevel::Low,
        requires_confirmation: false,
        reason: Some("create then focus".to_string()),
    };

    let compiled =
        compile_voice_plan_for_card_task(&request, plan).expect("合法可执行 VoicePlan 应编译成功");

    assert_eq!(
        compiled.commands.len(),
        2,
        "两步 VoicePlan 应编译成两条 executable commands"
    );
    assert!(
        matches!(compiled.commands[0], voice::ExecutableCommand::Card(_)),
        "第一步应编译成可执行的 create card command"
    );
    assert!(
        matches!(compiled.commands[1], voice::ExecutableCommand::Card(_)),
        "Focused 目标应编译成 existing-card focus command"
    );
    assert!(
        !compiled.requires_confirmation,
        "当前低风险非 submit 计划不应被误标成 requires_confirmation"
    );
}

/// CP-4：低置信 planner 结果应返回 TranscriptOnly rejection，而不是伪造执行计划。
///
/// 数据构造（含关键数值的推导过程）：
///   request.max_steps = 2
///   commands = 1
///   confidence = 0.20 < MIN_EXECUTE_CONFIDENCE(0.65)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造单步低置信 VoicePlan
///   2. 调用 compile_voice_plan_for_card_task(request, plan)
///   3. 读取错误枚举
///
/// 预期结果：
///   - 正断言：返回 TranscriptOnly error
///   - 负断言：低置信结果不应继续产出 compiled commands
#[test]
fn test_compile_voice_plan_for_card_task_rejects_transcript_only_outcome() {
    let request = planning_request(2);
    let plan = VoicePlan {
        commands: vec![PlannedCommand::Focus {
            target: PlannedTarget::Focused,
        }],
        confidence: 0.2,
        risk: RiskLevel::Low,
        requires_confirmation: false,
        reason: Some("too uncertain".to_string()),
    };

    let error = compile_voice_plan_for_card_task(&request, plan)
        .expect_err("低置信 VoicePlan 必须以 transcript-only 被拒绝");

    assert!(
        matches!(error, CardTaskPlannerError::TranscriptOnly { .. }),
        "低置信 VoicePlan 必须映射成 TranscriptOnly rejection"
    );
    assert!(
        !matches!(error, CardTaskPlannerError::Compile { .. }),
        "低置信 VoicePlan 不应伪装成 compile schema rejection"
    );
}
