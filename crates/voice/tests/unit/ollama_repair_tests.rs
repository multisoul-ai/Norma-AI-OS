use super::*;
use crate::{CardRef, VoiceCardKind, VoicePlan};

fn existing_cards() -> Vec<CardRef> {
    vec![
        CardRef {
            node_id: 17,
            title: "Codex".to_string(),
            alias: None,
            kind: VoiceCardKind::Agent {
                profile: AgentProfile::Codex,
            },
            selected: false,
            focused: false,
        },
        CardRef {
            node_id: 18,
            title: "Claude".to_string(),
            alias: None,
            kind: VoiceCardKind::Agent {
                profile: AgentProfile::Claude,
            },
            selected: false,
            focused: false,
        },
        CardRef {
            node_id: 19,
            title: "Terminal".to_string(),
            alias: None,
            kind: VoiceCardKind::Terminal,
            selected: false,
            focused: false,
        },
        CardRef {
            node_id: 20,
            title: "Terminal".to_string(),
            alias: None,
            kind: VoiceCardKind::Terminal,
            selected: false,
            focused: false,
        },
    ]
}

fn multi_create_request(cards: Vec<CardRef>) -> VoicePlanningRequest {
    VoicePlanningRequest {
        transcript: "打开codex,claude code, 再开启两个终端 然后输入hi".to_string(),
        cards,
        focused_node_id: None,
        selected_node_ids: Vec::new(),
        max_steps: 8,
    }
}

/// OR-1: 空画布复合语音应生成 Codex、Claude、两个 Terminal，并把输入发给最后一个 Terminal。
///
/// 数据构造（含关键数值的推导过程）：
///   transcript 显式包含 4 个卡片实体：Codex(1) + Claude Code(1) + 两个终端(2)
///   send_text = 1 条，因为 “然后输入hi” 是一个输入动作
///   Total commands = 1 + 1 + 2 + 1 = 5，低于 max_steps(8)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. required_plan_hint_json 解析空画布 transcript
///   2. Codex/Claude/两个终端分别转成独立 create 命令
///   3. 输入 hi 目标选择最近创建的兼容卡片，也就是第二个 Terminal 的 step 3
///
/// 预期结果：
///   - 断言 A：计划包含 5 条命令，覆盖全部显式实体和输入动作
///   - 断言 B：第 0 条是 Codex，第 1 条是 Claude，不能漏掉 Codex
///   - 断言 C：第 2、3 条都是 Terminal，不能把两个终端合并成一个
///   - 断言 D：send_text 指向 previous_step 3，而不是错误的 0 或 2
#[test]
fn test_required_hint_extracts_codex_claude_two_terminals_then_hi() {
    let request = multi_create_request(Vec::new());

    let hint = required_plan_hint_json(&request);
    let plan: VoicePlan = serde_json::from_value(hint).expect("required hint 应是 VoicePlan");

    assert_eq!(
        plan.commands.len(),
        5,
        "Codex + Claude + 两个 Terminal + send_text 应合计 5 条命令"
    );
    assert_eq!(
        plan.commands[0],
        PlannedCommand::Create {
            kind: PlannedCardKind::Agent {
                profile: AgentProfile::Codex,
            },
        },
        "第 0 条必须创建 Codex，不能被 Claude 覆盖或漏掉"
    );
    assert_eq!(
        plan.commands[1],
        PlannedCommand::Create {
            kind: PlannedCardKind::Agent {
                profile: AgentProfile::Claude,
            },
        },
        "第 1 条必须创建 Claude Code 对应的 Claude profile"
    );
    assert_eq!(
        plan.commands[2],
        PlannedCommand::Create {
            kind: PlannedCardKind::Terminal,
        },
        "第 2 条必须创建第一个 Terminal"
    );
    assert_eq!(
        plan.commands[3],
        PlannedCommand::Create {
            kind: PlannedCardKind::Terminal,
        },
        "第 3 条必须创建第二个 Terminal，不能合并计数"
    );
    assert_eq!(
        plan.commands[4],
        PlannedCommand::SendText {
            target: PlannedTarget::PreviousStep { step: 3 },
            text: "hi".to_string(),
            submit: false,
        },
        "输入 hi 应发给最后创建的 Terminal，也就是 previous_step 3"
    );
    assert_ne!(
        plan.commands[4],
        PlannedCommand::SendText {
            target: PlannedTarget::PreviousStep { step: 0 },
            text: "hi".to_string(),
            submit: false,
        },
        "send_text 不应错误回指 Codex 的 step 0"
    );
}

/// OR-2: 重复说“打开/开启”时，即使已有同类卡片，也应继续创建新的四张卡片。
///
/// 数据构造（含关键数值的推导过程）：
///   existing cards = 4，来自第一次成功创建的 Codex/Claude/Terminal/Terminal
///   transcript 再次显式要求打开 Codex(1)、Claude Code(1)、两个终端(2)
///   Total commands = 新建 4 张 + send_text 1 条 = 5，仍低于 max_steps(8)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造带 4 张已有卡片的 VoicePlanningRequest
///   2. required_plan_hint_json 不应因为 cards 非空返回 null
///   3. 解析 hint，检查它仍是 create/create/create/create/send_text
///
/// 预期结果：
///   - 断言 A：hint 不是 null，否则第二次输入会回到小模型坏计划
///   - 断言 B：第 0 条仍创建 Codex，说明已有 Codex 不会吞掉新建意图
///   - 断言 C：总命令数仍是 5，说明两个终端没有被已有终端抵消
#[test]
fn test_required_hint_repeats_explicit_open_when_cards_already_exist() {
    let request = multi_create_request(existing_cards());

    let hint = required_plan_hint_json(&request);
    let plan: VoicePlan = serde_json::from_value(hint).expect("重复打开时 hint 应是 VoicePlan");

    assert!(
        !plan.commands.is_empty(),
        "已有卡片时再次说打开/开启仍应生成 required hint"
    );
    assert_eq!(
        plan.commands[0],
        PlannedCommand::Create {
            kind: PlannedCardKind::Agent {
                profile: AgentProfile::Codex,
            },
        },
        "已有 Codex 时再次打开 codex 应创建新的 Codex 卡片"
    );
    assert_eq!(
        plan.commands.len(),
        5,
        "重复打开 Codex/Claude/两个终端并输入 hi 仍应得到 5 条命令"
    );
}

/// OR-3: 当小模型漏掉实体且引用未来 step 时，repair 应替换为确定性完整计划。
///
/// 数据构造（含关键数值的推导过程）：
///   bad plan = 3 条：create Claude(0) + create Terminal(1) + send previous_step 2
///   previous_step 2 在第 2 条命令处尚不是已创建卡片，因此 app 编译会拒绝
///   required plan = 5 条：Codex(0) + Claude(1) + Terminal(2) + Terminal(3) + send step 3
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造日志中出现的 schema-valid 但语义错误的 bad plan
///   2. repair_plan_with_required_hint 根据同一 transcript 生成 required plan
///   3. 因 bad plan 与 required plan 命令不一致，返回 repaired=true 和 required plan
///
/// 预期结果：
///   - 断言 A：repaired=true，说明坏计划被替换
///   - 断言 B：修复后包含 5 条命令，覆盖被模型漏掉的 Codex 和第二个 Terminal
///   - 断言 C：最后 send_text 指向 step 3，不再引用无效的 step 2
#[test]
fn test_repair_replaces_missing_entities_and_invalid_previous_step() {
    let request = multi_create_request(Vec::new());
    let bad_plan = VoicePlan {
        commands: vec![
            PlannedCommand::Create {
                kind: PlannedCardKind::Agent {
                    profile: AgentProfile::Claude,
                },
            },
            PlannedCommand::Create {
                kind: PlannedCardKind::Terminal,
            },
            PlannedCommand::SendText {
                target: PlannedTarget::PreviousStep { step: 2 },
                text: "hi".to_string(),
                submit: false,
            },
        ],
        confidence: 0.9,
        risk: RiskLevel::Low,
        requires_confirmation: false,
        reason: Some("Create Claude, terminal, and send hi without submitting".to_string()),
    };

    let (repaired_plan, repaired) = repair_plan_with_required_hint(&request, bad_plan);

    assert!(
        repaired,
        "模型漏掉 Codex/第二个 Terminal 且引用 step 2 时应触发 repair"
    );
    assert_eq!(
        repaired_plan.commands.len(),
        5,
        "修复后的计划应恢复为 5 条完整命令"
    );
    assert_eq!(
        repaired_plan.commands[0],
        PlannedCommand::Create {
            kind: PlannedCardKind::Agent {
                profile: AgentProfile::Codex,
            },
        },
        "修复后的第 0 条必须补回 Codex"
    );
    assert_eq!(
        repaired_plan.commands[4],
        PlannedCommand::SendText {
            target: PlannedTarget::PreviousStep { step: 3 },
            text: "hi".to_string(),
            submit: false,
        },
        "修复后的 send_text 必须指向已创建的第二个 Terminal step 3"
    );
    assert_ne!(
        repaired_plan.commands.len(),
        3,
        "坏计划的 3 条命令不应原样保留"
    );
}

/// OR-4: 没有打开/创建意图的已有卡片输入，不应被确定性 repair 改写成新建卡片。
///
/// 数据构造（含关键数值的推导过程）：
///   cards = 4，表示当前已有 Codex/Claude/Terminal/Terminal
///   transcript = “给codex发送hi”，只有发送意图，没有 打开/开启/创建 动词
///   original plan = 1 条 focus selected，用来检测 repair 是否误替换
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造已有卡片请求，但 transcript 不包含显式新建动词
///   2. required_plan_hint_json 应返回 null
///   3. repair_plan_with_required_hint 应保留 original plan
///
/// 预期结果：
///   - 断言 A：hint 是 null，说明非新建语音不进入确定性创建流
///   - 断言 B：repaired=false，现有上下文计划不被替换
///   - 断言 C：原始 focus selected 命令仍保留
#[test]
fn test_required_hint_skips_existing_card_send_without_create_intent() {
    let request = VoicePlanningRequest {
        transcript: "给codex发送hi".to_string(),
        cards: existing_cards(),
        focused_node_id: None,
        selected_node_ids: Vec::new(),
        max_steps: 8,
    };
    let original_plan = VoicePlan {
        commands: vec![PlannedCommand::Focus {
            target: PlannedTarget::Selected,
        }],
        confidence: 0.9,
        risk: RiskLevel::Low,
        requires_confirmation: false,
        reason: Some("focus selected".to_string()),
    };

    let hint = required_plan_hint_json(&request);
    let (plan, repaired) = repair_plan_with_required_hint(&request, original_plan.clone());

    assert_eq!(
        hint,
        Value::Null,
        "没有打开/创建动词时不应生成创建卡片 required hint"
    );
    assert!(!repaired, "已有卡片发送语音不应被替换成新建卡片计划");
    assert_eq!(
        plan.commands, original_plan.commands,
        "原始 focus selected 命令应被保留"
    );
}
