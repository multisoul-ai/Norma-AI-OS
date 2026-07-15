use agent::{
    dialogue_system_prompt, dialogue_tool_specs, parse_dialogue_tool_call, AgentMessage,
    AgentProvider, DialogueToolAction, MessageRole, OpenAiCompatibleProvider, ProviderConfig,
    ProviderProtocol, ProviderRequest, CARD_TASK_TOOL,
};

const BASE_URL_ENV: &str = "NORMA_OPENAI_COMPLIANCE_BASE_URL";
const MODEL_ENV: &str = "NORMA_OPENAI_COMPLIANCE_MODEL";
const API_KEY_ENV: &str = "NORMA_OPENAI_COMPLIANCE_API_KEY";
const REPETITIONS_ENV: &str = "NORMA_OPENAI_COMPLIANCE_REPETITIONS";

/// RPC-1：真实 OpenAI-compatible Dialogue Provider 必须对画布任务保持高召回，同时不误派发普通聊天。
///
/// 数据构造：从环境变量读取真实 Provider 配置；任务集覆盖明确创建、取消、状态查询，负样本覆盖普通聊天、设计讨论与含糊表达。
/// 执行过程：每条语料独立发送完整 system prompt、用户消息与生产 `card_task` schema；可通过 repetitions 重复采样。
/// 预期结果：明确创建、取消、状态查询分别返回合法动作，普通/含糊负样本零 `card_task`；输出召回率与误派发率。
///
/// 运行示例：
/// `NORMA_OPENAI_COMPLIANCE_BASE_URL=https://api.minimax.io/v1 NORMA_OPENAI_COMPLIANCE_MODEL=MiniMax-M2.5 NORMA_OPENAI_COMPLIANCE_API_KEY=... cargo test -p agent --test real_provider_compliance_tests -- --ignored --nocapture`
#[test]
#[ignore = "requires an explicitly configured real OpenAI-compatible Provider"]
fn real_dialogue_provider_meets_card_task_recall_and_false_dispatch_contract() {
    let config = compliance_config();
    let repetitions = std::env::var(REPETITIONS_ENV)
        .ok()
        .map(|value| {
            value
                .parse::<usize>()
                .expect("NORMA_OPENAI_COMPLIANCE_REPETITIONS 必须是正整数")
        })
        .unwrap_or(1);
    assert!(repetitions > 0, "真实 Provider 合规测试至少需要一次采样");

    let create_cases = [
        "请创建一个终端卡片",
        "在画布上新建一个 Codex Agent 卡片",
        "帮我打开一个终端并运行 pwd",
        "创建 Claude 卡片，让它分析 README",
    ];
    let cancel_cases = ["取消任务 7", "取消编号为 42 的卡片任务"];
    let status_cases = ["刚才的卡片任务进度如何", "查询最新卡片任务的状态"];
    let negative_cases = [
        "你好，今天怎么样",
        "如何创建终端卡片？",
        "我们讨论一下终端卡片的交互设计",
        "解释一下 Codex 和 Claude 的区别",
    ];
    let ambiguous_cases = ["帮我处理一下画布", "把刚才那张卡片弄一下"];

    let mut create_hits = 0_usize;
    let mut cancel_hits = 0_usize;
    let mut status_hits = 0_usize;
    let mut false_dispatches = 0_usize;
    for _ in 0..repetitions {
        for input in create_cases {
            let actions = provider_actions(config.clone(), input);
            if actions
                .iter()
                .any(|action| matches!(action, DialogueToolAction::Start { .. }))
            {
                create_hits += 1;
            }
        }
        for input in cancel_cases {
            let actions = provider_actions(config.clone(), input);
            if actions
                .iter()
                .any(|action| matches!(action, DialogueToolAction::Cancel { .. }))
            {
                cancel_hits += 1;
            }
        }
        for input in status_cases {
            let actions = provider_actions(config.clone(), input);
            if actions
                .iter()
                .any(|action| matches!(action, DialogueToolAction::Status { .. }))
            {
                status_hits += 1;
            }
        }
        for input in negative_cases {
            if !provider_actions(config.clone(), input).is_empty() {
                false_dispatches += 1;
            }
        }
        for input in ambiguous_cases {
            if !provider_actions(config.clone(), input).is_empty() {
                false_dispatches += 1;
            }
        }
    }

    let create_total = create_cases.len() * repetitions;
    let cancel_total = cancel_cases.len() * repetitions;
    let status_total = status_cases.len() * repetitions;
    let negative_total = (negative_cases.len() + ambiguous_cases.len()) * repetitions;
    eprintln!(
        "real provider compliance: create_recall={create_hits}/{create_total}, cancel_recall={cancel_hits}/{cancel_total}, status_recall={status_hits}/{status_total}, false_dispatch={false_dispatches}/{negative_total}"
    );
    assert_eq!(
        create_hits, create_total,
        "真实 Dialogue Provider 必须对明确创建语料达到 100% card_task start 召回"
    );
    assert_eq!(
        cancel_hits, cancel_total,
        "真实 Dialogue Provider 必须对明确取消语料达到 100% card_task cancel 召回"
    );
    assert_eq!(
        status_hits, status_total,
        "真实 Dialogue Provider 必须对明确状态查询达到 100% card_task status 召回"
    );
    assert_eq!(
        false_dispatches, 0,
        "真实 Dialogue Provider 不得对普通聊天、设计讨论或含糊表达误调用 card_task"
    );
}

fn compliance_config() -> ProviderConfig {
    let base_url = required_env(BASE_URL_ENV);
    let model = required_env(MODEL_ENV);
    let api_key = required_env(API_KEY_ENV);
    ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        base_url,
        model,
        Some(api_key),
    )
}

fn required_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("运行真实 Provider 合规测试前必须设置 {name}"))
}

fn provider_actions(config: ProviderConfig, input: &str) -> Vec<DialogueToolAction> {
    let mut provider = OpenAiCompatibleProvider::new(config);
    let request = ProviderRequest {
        messages: vec![
            AgentMessage {
                role: MessageRole::System,
                content: dialogue_system_prompt(),
                tool_call_id: None,
                tool_calls: Vec::new(),
            },
            AgentMessage {
                role: MessageRole::User,
                content: input.to_string(),
                tool_call_id: None,
                tool_calls: Vec::new(),
            },
        ],
        tools: dialogue_tool_specs(),
    };
    let response = provider
        .stream(&request, &mut |_| {})
        .unwrap_or_else(|error| panic!("真实 Provider 请求失败，输入={input:?}，错误={error}"));
    response
        .tool_calls
        .iter()
        .filter(|call| call.name == CARD_TASK_TOOL)
        .map(|call| {
            parse_dialogue_tool_call(&call.arguments_json).unwrap_or_else(|error| {
                panic!(
                    "真实 Provider 返回非法 card_task 参数，输入={input:?}，参数={:?}，错误={error}",
                    call.arguments_json
                )
            })
        })
        .collect()
}
