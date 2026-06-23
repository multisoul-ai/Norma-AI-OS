use super::*;
use norma::protocol::{AgentProfile, CardCommand, CardKind, Placement};

fn request_with_cards(cards: Vec<CardRef>) -> VoicePlanningRequest {
    VoicePlanningRequest {
        transcript: "Alan, continue the plan".to_string(),
        cards,
        focused_node_id: None,
        selected_node_ids: Vec::new(),
        max_steps: 2,
    }
}

fn request_with_max_steps(max_steps: usize) -> VoicePlanningRequest {
    VoicePlanningRequest {
        transcript: "打开codex,claude code, 再开启两个终端 然后输入hi".to_string(),
        cards: Vec::new(),
        focused_node_id: None,
        selected_node_ids: Vec::new(),
        max_steps,
    }
}

fn agent_card(node_id: u64, title: &str, alias: Option<&str>) -> CardRef {
    CardRef {
        node_id,
        title: title.to_string(),
        alias: alias.map(str::to_string),
        kind: VoiceCardKind::Agent {
            profile: AgentProfile::Codex,
        },
        selected: false,
        focused: false,
    }
}

fn terminal_card(node_id: u64) -> CardRef {
    CardRef {
        node_id,
        title: "Terminal".to_string(),
        alias: None,
        kind: VoiceCardKind::Terminal,
        selected: false,
        focused: false,
    }
}

/// V-1: create -> send 两步计划应编译为创建命令和 previous-step 发送命令。
///
/// 数据构造（含关键数值的推导过程）：
///   max_steps = 2
///   plan.commands = 2，因此 2 <= max_steps
///   confidence = 0.91，高于 MIN_EXECUTE_CONFIDENCE(0.65)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 第 0 步 Create Agent(Codex) → CardCommand::Create
///   2. 第 1 步 SendText target=PreviousStep(0) → SendTextToPrevious(0)
///   3. previous step 0 是 create，引用合法
///
/// 预期结果：
///   - 正断言：第一步是创建 Codex Agent 的 CardCommand
///   - 正断言：第二步发送到 previous_step=0，保留文本和 submit=false
///   - 负断言：该计划不应要求确认，因为没有 terminal submit 或 high risk
#[test]
fn test_compile_create_then_send_to_previous_step() {
    let request = request_with_cards(Vec::new());
    let plan = VoicePlan {
        commands: vec![
            PlannedCommand::Create {
                kind: PlannedCardKind::Agent {
                    profile: AgentProfile::Codex,
                },
            },
            PlannedCommand::SendText {
                target: PlannedTarget::PreviousStep { step: 0 },
                text: "continue the plan".to_string(),
                submit: false,
            },
        ],
        confidence: 0.91,
        risk: RiskLevel::Low,
        requires_confirmation: false,
        reason: Some("create codex and send prompt".to_string()),
    };

    let decision = compile_voice_plan(&request, plan).expect("合法两步计划应编译成功");
    let VoicePlanDecision::Execute(compiled) = decision else {
        panic!("高置信合法计划不应降级为 transcript-only");
    };

    assert_eq!(
        compiled.commands[0],
        ExecutableCommand::Card(CardCommand::Create {
            kind: CardKind::Agent {
                profile: AgentProfile::Codex,
            },
            placement: Placement::VisibleArea,
        }),
        "第一步应编译为创建 Codex Agent 的受限 CardCommand"
    );
    assert_eq!(
        compiled.commands[1],
        ExecutableCommand::SendTextToPrevious {
            previous_step: 0,
            text: "continue the plan".to_string(),
            submit: false,
        },
        "第二步应发送到第 0 步新建卡片，而不是猜测固定 node_id"
    );
    assert!(
        !compiled.requires_confirmation,
        "Agent send 且 low risk 时不应要求确认"
    );
}

/// V-2: 四个 create 后再 send_text 的复合语音应完整编译，不应被旧两步限制截断。
///
/// 数据构造（含关键数值的推导过程）：
///   Codex Agent create      = 1 command
///   Claude Agent create     = 1 command
///   Terminal create         = 2 commands
///   SendText(previous 3)    = 1 command
///   Total                   = 1 + 1 + 2 + 1 = 5 commands
///   max_steps               = 5，因此 Total == max_steps
///   confidence              = 0.93，高于 MIN_EXECUTE_CONFIDENCE(0.65)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 第 0 步创建 Codex Agent
///   2. 第 1 步创建 Claude Agent（Claude Code 归一到 Claude profile）
///   3. 第 2/3 步创建两个 Terminal
///   4. 第 4 步 SendText target=PreviousStep(3)，把 hi 输入最后创建的 Terminal
///
/// 预期结果：
///   - 正断言：编译结果保留 5 条命令，前四条分别创建 Codex、Claude、Terminal、Terminal
///   - 正断言：最后一步发送 hi 到 previous_step=3 且 submit=false
///   - 负断言：不应只剩 1 条命令，也不应要求确认
#[test]
fn test_compile_multi_create_then_send_to_last_created_terminal() {
    let request = request_with_max_steps(5);
    let plan = VoicePlan {
        commands: vec![
            PlannedCommand::Create {
                kind: PlannedCardKind::Agent {
                    profile: AgentProfile::Codex,
                },
            },
            PlannedCommand::Create {
                kind: PlannedCardKind::Agent {
                    profile: AgentProfile::Claude,
                },
            },
            PlannedCommand::Create {
                kind: PlannedCardKind::Terminal,
            },
            PlannedCommand::Create {
                kind: PlannedCardKind::Terminal,
            },
            PlannedCommand::SendText {
                target: PlannedTarget::PreviousStep { step: 3 },
                text: "hi".to_string(),
                submit: false,
            },
        ],
        confidence: 0.93,
        risk: RiskLevel::Low,
        requires_confirmation: false,
        reason: Some("create four cards and type hi in the last terminal".to_string()),
    };

    let decision = compile_voice_plan(&request, plan).expect("五步复合语音应可编译成功");
    let VoicePlanDecision::Execute(compiled) = decision else {
        panic!("高置信五步复合语音不应降级为 transcript-only");
    };

    assert_eq!(
        compiled.commands.len(),
        5,
        "复合语音应保留 4 个 create + 1 个 send_text，共 5 条命令"
    );
    assert_eq!(
        compiled.commands[0],
        ExecutableCommand::Card(CardCommand::Create {
            kind: CardKind::Agent {
                profile: AgentProfile::Codex,
            },
            placement: Placement::VisibleArea,
        }),
        "第 0 步应创建 Codex Agent 卡片"
    );
    assert_eq!(
        compiled.commands[1],
        ExecutableCommand::Card(CardCommand::Create {
            kind: CardKind::Agent {
                profile: AgentProfile::Claude,
            },
            placement: Placement::VisibleArea,
        }),
        "第 1 步应创建 Claude Agent 卡片，用于承接 Claude Code 语义"
    );
    assert_eq!(
        compiled.commands[2],
        ExecutableCommand::Card(CardCommand::Create {
            kind: CardKind::Terminal { shell: None },
            placement: Placement::VisibleArea,
        }),
        "第 2 步应创建第一个 Terminal 卡片"
    );
    assert_eq!(
        compiled.commands[3],
        ExecutableCommand::Card(CardCommand::Create {
            kind: CardKind::Terminal { shell: None },
            placement: Placement::VisibleArea,
        }),
        "第 3 步应创建第二个 Terminal 卡片"
    );
    assert_eq!(
        compiled.commands[4],
        ExecutableCommand::SendTextToPrevious {
            previous_step: 3,
            text: "hi".to_string(),
            submit: false,
        },
        "最后一步应把 hi 输入到第 3 步创建的最后一个 Terminal"
    );
    assert_ne!(
        compiled.commands.len(),
        1,
        "复合语音不应被旧限制压缩成单条 create 命令"
    );
    assert!(
        !compiled.requires_confirmation,
        "Terminal submit=false 只是输入文本，不应要求确认"
    );
}

/// V-3: 超过 max_steps 的计划必须拒绝，避免一次语音执行复杂编排。
///
/// 数据构造（含关键数值的推导过程）：
///   max_steps = 2
///   plan.commands = 3，因此 3 > 2
///   confidence = 0.99 不应绕过步数上限
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 3 个 Create Terminal 命令
///   2. compile_voice_plan 检查 commands.len()
///   3. 返回 TooManySteps
///
/// 预期结果：
///   - 正断言：错误是 TooManySteps { actual: 3, max: 2 }
///   - 负断言：不应返回 Execute，否则 LLM 可越权批量操作
#[test]
fn test_compile_rejects_more_than_two_steps() {
    let request = request_with_cards(Vec::new());
    let plan = VoicePlan {
        commands: vec![
            PlannedCommand::Create {
                kind: PlannedCardKind::Terminal,
            },
            PlannedCommand::Create {
                kind: PlannedCardKind::Terminal,
            },
            PlannedCommand::Create {
                kind: PlannedCardKind::Terminal,
            },
        ],
        confidence: 0.99,
        risk: RiskLevel::Low,
        requires_confirmation: false,
        reason: None,
    };

    let error = compile_voice_plan(&request, plan).expect_err("三步计划必须被拒绝");

    assert_eq!(
        error,
        VoicePlanError::TooManySteps { actual: 3, max: 2 },
        "超过 max_steps 时应返回精确 TooManySteps 错误"
    );
    assert_ne!(
        error,
        VoicePlanError::EmptyPlan,
        "三步计划不是空计划，不应报 EmptyPlan 掩盖越界原因"
    );
}

/// V-4: alias 唯一命中时应发送到对应 Agent，重名时必须拒绝。
///
/// 数据构造（含关键数值的推导过程）：
///   唯一 alias: node_id=7, alias=Alan
///   歧义 alias: node_id=7 与 node_id=8 都 alias=Alan
///   confidence = 0.84，高于 0.65
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 对唯一 alias 请求编译 SendText(alias=Alan)
///   2. 对重复 alias 请求编译同一计划
///   3. 比较成功目标与失败错误
///
/// 预期结果：
///   - 正断言：唯一 alias 编译为 SendText node_id=7
///   - 正断言：重复 alias 返回 AliasAmbiguous
///   - 负断言：重复 alias 不应静默选择第一张卡片
#[test]
fn test_alias_resolution_sends_only_when_unique() {
    let unique_request = request_with_cards(vec![agent_card(7, "Codex", Some("Alan"))]);
    let ambiguous_request = request_with_cards(vec![
        agent_card(7, "Codex", Some("Alan")),
        agent_card(8, "Claude", Some("Alan")),
    ]);
    let plan = VoicePlan {
        commands: vec![PlannedCommand::SendText {
            target: PlannedTarget::Alias {
                alias: "Alan".to_string(),
            },
            text: "continue".to_string(),
            submit: false,
        }],
        confidence: 0.84,
        risk: RiskLevel::Low,
        requires_confirmation: false,
        reason: None,
    };

    let unique = compile_voice_plan(&unique_request, plan.clone()).expect("唯一 alias 应成功");
    let VoicePlanDecision::Execute(compiled) = unique else {
        panic!("唯一 alias 高置信计划不应降级");
    };
    let ambiguous = compile_voice_plan(&ambiguous_request, plan).expect_err("重复 alias 必须失败");

    assert_eq!(
        compiled.commands[0],
        ExecutableCommand::Card(CardCommand::SendText {
            node_id: 7,
            text: "continue".to_string(),
            submit: false,
        }),
        "alias=Alan 唯一命中时应发送到 node_id=7"
    );
    assert_eq!(
        ambiguous,
        VoicePlanError::AliasAmbiguous {
            alias: "Alan".to_string(),
        },
        "重复 alias 必须返回 AliasAmbiguous"
    );
    assert_ne!(
        ambiguous,
        VoicePlanError::AliasNotFound {
            alias: "Alan".to_string(),
        },
        "重复 alias 已命中多张卡片，不应误报未找到"
    );
}

/// V-5: Terminal submit 必须要求确认，Terminal 非 submit 可以直发文本。
///
/// 数据构造（含关键数值的推导过程）：
///   terminal node_id = 3
///   submit=false：只是写入文本，不执行命令
///   submit=true：会追加换行并执行，因此风险升级
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 编译 submit=false 的 Terminal SendText
///   2. 编译 submit=true 的 Terminal SendText
///   3. 比较 requires_confirmation
///
/// 预期结果：
///   - 正断言：submit=false 不要求确认
///   - 正断言：submit=true 要求确认
///   - 负断言：submit=true 不应被当成普通低风险直发
#[test]
fn test_terminal_submit_requires_confirmation_but_plain_text_does_not() {
    let request = request_with_cards(vec![terminal_card(3)]);
    let plain = VoicePlan {
        commands: vec![PlannedCommand::SendText {
            target: PlannedTarget::NodeId { node_id: 3 },
            text: "ls".to_string(),
            submit: false,
        }],
        confidence: 0.9,
        risk: RiskLevel::Low,
        requires_confirmation: false,
        reason: None,
    };
    let submit = VoicePlan {
        commands: vec![PlannedCommand::SendText {
            target: PlannedTarget::NodeId { node_id: 3 },
            text: "rm -rf target".to_string(),
            submit: true,
        }],
        confidence: 0.9,
        risk: RiskLevel::Low,
        requires_confirmation: false,
        reason: None,
    };

    let VoicePlanDecision::Execute(plain) =
        compile_voice_plan(&request, plain).expect("plain terminal text 应可编译")
    else {
        panic!("plain terminal text 不应降级");
    };
    let VoicePlanDecision::Execute(submit) =
        compile_voice_plan(&request, submit).expect("terminal submit 应可编译但要确认")
    else {
        panic!("terminal submit 不应降级");
    };

    assert!(
        !plain.requires_confirmation,
        "Terminal submit=false 只是填入文本，不应要求确认"
    );
    assert!(
        submit.requires_confirmation,
        "Terminal submit=true 会执行命令，必须要求确认"
    );
    assert_ne!(
        plain.requires_confirmation, submit.requires_confirmation,
        "submit=true 与 submit=false 的风险处理应不同"
    );
}

/// V-6: 低置信 planner 输出应降级为 transcript-only，不执行命令。
///
/// 数据构造（含关键数值的推导过程）：
///   confidence = 0.40
///   MIN_EXECUTE_CONFIDENCE = 0.65
///   0.40 < 0.65，因此无论命令看起来是否合理都不能执行
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造低置信 Create Terminal plan
///   2. compile_voice_plan 检查 confidence
///   3. 返回 TranscriptOnly
///
/// 预期结果：
///   - 正断言：结果是 TranscriptOnly
///   - 正断言：reason 包含 confidence
///   - 负断言：不应返回 Execute
#[test]
fn test_low_confidence_plan_becomes_transcript_only() {
    let request = request_with_cards(Vec::new());
    let plan = VoicePlan {
        commands: vec![PlannedCommand::Create {
            kind: PlannedCardKind::Terminal,
        }],
        confidence: 0.40,
        risk: RiskLevel::Low,
        requires_confirmation: false,
        reason: None,
    };

    let decision = compile_voice_plan(&request, plan).expect("低置信应正常降级而不是报错");

    match decision {
        VoicePlanDecision::TranscriptOnly { reason } => {
            assert!(
                reason.contains("confidence"),
                "低置信降级原因应说明 confidence，实际: {reason}"
            );
        }
        VoicePlanDecision::Execute(_) => {
            panic!("低置信计划不应返回 Execute，否则会执行不可靠命令")
        }
    }
}
