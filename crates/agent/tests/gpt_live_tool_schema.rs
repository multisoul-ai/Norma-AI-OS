#[path = "support/sse.rs"]
mod sse;

use agent::{
    AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, NormaDelegation, NormaSession,
    ProviderConfig, ProviderProtocol, ToolContext,
};
use futures::future::BoxFuture;
use norma::protocol::AgentProfile;
use serde_json::json;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Host(Mutex<Vec<AdapterCommand>>);

impl AgentCardAdapter for Host {
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            let mut calls = self.0.lock().expect("记录真实typed工具请求");
            calls.push(command);
            Ok(
                AdapterResult::succeeded("本地宿主已创建卡片")
                    .with_node_id(40 + calls.len() as u64),
            )
        })
    }
}

/// 场景：用户语音要求创建一个 Codex 和一个 Claude Code，不能被删除工具的定义阻断。
/// 构造：标准指令及真实日志中的口语原话；独立HTTP夹具校验全部工具，再返回两次创建。
/// 执行：公开turn_delegated→真实Rig HTTP请求→typed Adapter→最终回执。
/// 预期：两轮请求均通过协议校验，且只创建Codex/Claude各一张，没有删除副作用。
#[tokio::test]
async fn gpt_live_creates_codex_and_claude_with_openai_compatible_tools() {
    for user_input in [
        "创建一个Codex和一个Claude Code智能体卡片",
        "喂喂创建两个智能体卡片嗯,你就帮我创建一下Codex跟- 那个 Claude Code这两个智能体卡片吧",
        "嗯，你就帮我创建一下Codex和Claude Code这两个智能体卡片吧",
        "你好，请帮我创建一个Codex和一个Claude Code智能体卡片",
        "嗯，创建Codex和Claude Code卡片，嗯不对，请帮我创建Codex和Claude Code卡片",
        "喂，帮我创建Codex和Claude Code卡片，备注“嗯算了”",
    ] {
        let (endpoint, server) = sse::serve_openai(vec![
        json!({"role":"assistant","tool_calls":[
            {"index":0,"id":"codex-card","type":"function","function":{"name":"create_agent","arguments":"{\"profile\":\"codex\"}"}},
            {"index":1,"id":"claude-card","type":"function","function":{"name":"create_agent","arguments":"{\"profile\":\"claude\"}"}}
        ]}),
        json!({"role":"assistant","content":"已创建两张卡片"}),
    ]).await;
        let host = Arc::new(Host::default());
        let config = ProviderConfig::new(
            ProviderProtocol::OpenAiCompatible,
            endpoint,
            "fixture",
            Some("fixture".into()),
        );
        let mut session =
            NormaSession::new("isolated-project", &config, host.clone()).expect("构造真实会话");
        let result = session
            .turn_delegated(NormaDelegation {
                request_id: "live-schema:create".into(),
                revision: 1,
                user_input: user_input.into(),
                conversation: String::new(),
                cancellation: Default::default(),
            })
            .await;
        let requests = server.await.expect("校验服务正常结束");
        let result = result.expect("GPT-Live全部工具定义应通过OpenAI校验，不能返回HTTP400");
        assert_eq!(requests.len(), 2, "工具请求和最终回复都必须通过校验");
        assert_eq!(result.tool_calls, 2, "用户要求的两项创建都应完成");
        assert_eq!(
            *host.0.lock().expect("宿主操作"),
            vec![
                AdapterCommand::CreateAgent {
                    profile: AgentProfile::Codex,
                    task: None
                },
                AdapterCommand::CreateAgent {
                    profile: AgentProfile::Claude,
                    task: None
                },
            ],
            "口语原话也应执行指定创建，不得混入删除或重复创建：{user_input}"
        );
    }
}

/// 场景：讨论、引用和撤回交给模型理解，本地不得自行推断并产生创建副作用。
/// 构造：原话包含多种讨论或撤回表达，模型只返回说明，不选择任何工具。
/// 执行：公开委派入口经真实HTTP，核对模型实际请求和宿主记录。
/// 预期：只有一轮模型请求，无工具调用和宿主副作用；不声称验证模型的语言理解质量。
#[tokio::test]
async fn gpt_live_without_model_tool_calls_has_no_local_creation_side_effects() {
    for input in [
        "嗯，你就不要创建Codex卡片",
        "喂喂，『创建Codex卡片』只是例句",
        "嗯，如果创建Codex卡片会怎么样",
        "喂，先分析创建Codex的方案",
        "嗯，创建Codex卡片好吗？",
        "喂，创建Codex卡片，算了",
        "嗯，创建一个Codex卡片，嗯算了",
        "嗯，创建一个Codex卡片，嗯取消刚才的请求",
        "嗯，你创建Codex卡片的操作很标准",
        "嗯，你帮我创建Codex卡片的操作很标准",
        "喂，创建Codex卡片这件事很简单",
        "嗯，创建Codex卡片很简单",
        "嗯，创建一个Codex卡片，别执行",
        "嗯，创建一个Codex卡片，嗯别创建",
        "嗯，创建一个Codex卡片，别执行刚才的创建",
        "嗯，创建一个Codex卡片，别创建Codex卡片了",
        "嗯，创建一个Codex卡片，别创建“Codex”",
        "喂喂",
    ] {
        let (endpoint, server) =
            sse::serve_openai(vec![json!({"role":"assistant","content":"未执行创建"})]).await;
        let host = Arc::new(Host::default());
        let config = ProviderConfig::new(
            ProviderProtocol::OpenAiCompatible,
            endpoint,
            "fixture",
            Some("fixture".into()),
        );
        let mut session =
            NormaSession::new("isolated-project", &config, host.clone()).expect("独立模型会话");
        let result = session
            .turn_delegated(NormaDelegation {
                request_id: "live-spoken:negative".into(),
                revision: 1,
                user_input: input.into(),
                conversation: "Assistant: 创建一个Codex卡片".into(),
                cancellation: Default::default(),
            })
            .await
            .expect("模型无工具回合应正常完成");
        let requests = server.await.expect("单轮HTTP完成");
        assert_eq!(
            result.tool_calls, 0,
            "模型未选择工具时本地不能自行执行：{input}"
        );
        assert!(
            host.0.lock().expect("宿主实际调用").is_empty(),
            "模型没有请求创建时，本地不能自行创建：{input}"
        );
        assert_eq!(
            requests.len(),
            1,
            "无工具回合不应被本地规则追加工具请求：{input}"
        );
    }
}

/// 场景：创建后的任务正文与备注可以包含否定或评价术语，不能被当作撤回。
/// 构造：明确创建Codex，任务包含术语解释或不执行代码的只读要求。
/// 执行：公开委派入口经HTTP及生产工具运行时，检查实际到达宿主的CreateAgent。
/// 预期：原话里的任务限制不阻断模型选择的工具，宿主采用模型给出的任务参数。
#[tokio::test]
async fn gpt_live_creation_task_payload_does_not_block_model_tools() {
    let command = AdapterCommand::CreateAgent {
        profile: AgentProfile::Codex,
        task: Some("解释给定术语".into()),
    };
    for input in [
        "帮我创建一个 Codex 智能体，分析如何优化登录",
        "喂，帮我创建Codex卡片来解释“别执行”这个词",
        "嗯，你帮我创建Codex卡片来解释“创建卡片的操作”这个短语",
        "创建一个Codex卡片，任务是只读审查，别执行代码",
        "创建一个Codex卡片，任务是只读审查，别执行“rm -rf”",
        "创建一个Codex卡片，任务是只读审查，别创建文件",
        "创建一个Codex卡片，任务是只读审查，别新建分支",
    ] {
        let (endpoint, server) = sse::serve_openai(vec![
            json!({"role":"assistant","tool_calls":[{"index":0,"id":"task-card","type":"function","function":{"name":"create_agent","arguments":"{\"profile\":\"codex\",\"task\":\"解释给定术语\"}"}}]}),
            json!({"role":"assistant","content":"已创建卡片并附带任务"}),
        ]).await;
        let host = Arc::new(Host::default());
        let config = ProviderConfig::new(
            ProviderProtocol::OpenAiCompatible,
            endpoint,
            "fixture",
            Some("fixture".into()),
        );
        let mut session =
            NormaSession::new("isolated-project", &config, host.clone()).expect("任务载荷会话");
        session
            .turn_delegated(NormaDelegation {
                request_id: "live-task:creation".into(),
                revision: 1,
                user_input: input.into(),
                conversation: String::new(),
                cancellation: Default::default(),
            })
            .await
            .expect("任务载荷不应阻断工具回合");
        let requests = server.await.expect("任务创建HTTP往返");
        assert_eq!(requests.len(), 2, "创建必须完成工具及最终回复两轮请求");
        assert_eq!(
            *host.0.lock().expect("实际任务创建"),
            vec![command.clone()],
            "创建目的中的载荷不应撤回指令：{input}"
        );
    }
}

/// 场景：手机实时会话复用公共授权入口，不能被GPT专用的口语规则改变。
/// 构造：相同Codex创建命令与标准句、带问候句；直接调用原公共接口。
/// 执行：分别判断两种原话，不经过GPT专用Adapter分支。
/// 预期：标准句继续允许，带口语前缀继续按原严格规则拒绝。
#[test]
fn non_gpt_authorization_retains_original_strict_creation_requests() {
    let command = AdapterCommand::CreateAgent {
        profile: AgentProfile::Codex,
        task: None,
    };
    assert!(
        agent::realtime_command_authorized(&command, "创建Codex卡片"),
        "公共入口的原标准创建指令应继续允许"
    );
    assert!(
        !agent::realtime_command_authorized(&command, "喂喂，帮我创建Codex卡片"),
        "公共入口不得继承GPT专用口语规则"
    );
}

/// 场景：既有普通语音、豆包来源和文字会话不得启用GPT的Schema适配。
/// 构造：三种原有来源；每次让真实HTTP客户端完成一次Terminal工具往返。
/// 执行：公开turn_streaming入口，逐项对比实际发送参数与未修改的公共注册表。
/// 预期：原有16项工具定义逐项一致，另有后台大脑交接工具 handoff_to_brain 共17项，删除工具仍有oneOf，且Terminal正常执行。
#[tokio::test]
async fn existing_voice_sources_keep_original_tool_definitions_and_execution() {
    for source in [
        agent::AgentInputSource::Voice,
        agent::AgentInputSource::DoubaoVoice,
        agent::AgentInputSource::Other,
    ] {
        let (endpoint, server) = sse::serve(vec![
            json!({"role":"assistant","tool_calls":[{"index":0,"id":"legacy-terminal","type":"function","function":{"name":"create_terminal","arguments":"{}"}}]}),
            json!({"role":"assistant","content":"已创建终端"}),
        ]).await;
        let host = Arc::new(Host::default());
        let config = ProviderConfig::new(
            ProviderProtocol::OpenAiCompatible,
            endpoint,
            "fixture",
            Some("fixture".into()),
        );
        let mut session =
            NormaSession::new("isolated-project", &config, host.clone()).expect("旧来源会话");
        let (deltas, _receiver) = tokio::sync::mpsc::channel(16);
        session
            .turn_streaming("创建一个终端", deltas, source)
            .await
            .expect("既有工具路径应继续成功");
        let requests = server.await.expect("原有请求完成");
        let tools = requests[0]["tools"].as_array().expect("原有工具列表");
        let definitions = agent::norma_tool_definitions();
        assert_eq!(
            tools.len(),
            17,
            "原有桌面来源工具集合除新增 handoff_to_brain 外不能增删；公共定义中workspace_memory仅手机使用"
        );
        for tool in tools {
            let name = tool["function"]["name"].as_str().expect("工具名称");
            let original = definitions
                .iter()
                .find(|definition| definition.name == name)
                .expect("来自公共注册表");
            assert_eq!(
                tool["function"]["parameters"], original.parameters,
                "{source:?}的{name}参数不得因GPT修复而变化"
            );
        }
        assert_eq!(
            *host.0.lock().expect("旧来源宿主操作"),
            vec![AdapterCommand::CreateTerminal],
            "原有来源仍只创建一次终端"
        );
        assert!(
            definitions
                .iter()
                .find(|definition| definition.name == "delete_canvas_card")
                .expect("公共删除工具")
                .parameters
                .get("oneOf")
                .is_some(),
            "不能改写公共删除Schema"
        );
    }
}

/// 场景：GPT移除线上的oneOf后，本地仍拒绝缺目标或多目标的删除请求。
/// 构造：空参数、node_id与position同时存在、以及合法node_id=2三种响应。
/// 执行：公开委派入口经真实HTTP校验，再进入原有工具参数及授权校验。
/// 预期：非法参数无副作用且返回具体原因；合法请求仍可到达宿主，避免以禁用工具假修复。
#[tokio::test]
async fn gpt_live_preserves_delete_target_validation_after_schema_adaptation() {
    for arguments in [
        json!({}),
        json!({"node_id":2,"position":2}),
        json!({"node_id":2}),
    ] {
        let valid = arguments == json!({"node_id":2});
        let (endpoint, server) = sse::serve_openai(vec![
            json!({"role":"assistant","tool_calls":[{"index":0,"id":"delete-target","type":"function","function":{"name":"delete_canvas_card","arguments":arguments.to_string()}}]}),
            json!({"role":"assistant","content":"已核对工具回执"}),
        ]).await;
        let host = Arc::new(Host::default());
        let config = ProviderConfig::new(
            ProviderProtocol::OpenAiCompatible,
            endpoint,
            "fixture",
            Some("fixture".into()),
        );
        let mut session =
            NormaSession::new("isolated-project", &config, host.clone()).expect("真实删除会话");
        session
            .turn_delegated(NormaDelegation {
                request_id: "live-schema:delete".into(),
                revision: 1,
                user_input: "删除卡片编号2".into(),
                conversation: String::new(),
                cancellation: Default::default(),
            })
            .await
            .expect("非法工具参数应以工具回执处理，不能再被线上Schema校验阻断");
        let requests = server.await.expect("校验和工具回执完成");
        let calls = host.0.lock().expect("实际删除请求");
        if valid {
            assert_eq!(
                *calls,
                vec![AdapterCommand::DeleteCanvasCard { node_id: 2 }],
                "合法单目标仍应执行一次"
            );
            assert!(
                !requests[1]["messages"]
                    .to_string()
                    .contains("requires exactly one"),
                "合法目标不得误报参数错误"
            );
        } else {
            assert!(calls.is_empty(), "缺目标或双目标不得执行任何删除");
            assert!(
                requests[1]["messages"]
                    .to_string()
                    .contains("requires exactly one"),
                "必须由原有执行器给出互斥目标错误"
            );
        }
    }
}

/// 场景：用明确提供的测试Key验证官方接口接受生产定义，并发出用户所需的两项创建。
/// 构造：固定官方地址和GPT-Live工具模型；隔离宿主只记录命令，不接入用户画布。
/// 执行：显式运行ignored测试，从环境读取Key，经公开委派入口完成真实云端往返。
/// 预期：官方模型调用Codex/Claude各一次，没有HTTP400或删除；不替代真人音频验收。
#[tokio::test]
#[ignore = "需要显式提供GPT_LIVE_TEST_API_KEY，会调用官方OpenAI测试账户"]
async fn gpt_live_official_openai_accepts_tools_and_requests_both_agents() {
    let secret = std::env::var("GPT_LIVE_TEST_API_KEY").expect("仅显式传入已授权的测试Key");
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        "https://api.openai.com/v1",
        "gpt-5.4-mini",
        Some(secret),
    );
    let host = Arc::new(Host::default());
    let mut session = NormaSession::new("isolated-openai-schema-probe", &config, host.clone())
        .expect("官方测试会话");
    let result = session
        .turn_delegated(NormaDelegation {
            request_id: "live-schema:official".into(),
            revision: 1,
            user_input: "创建一个Codex和一个Claude Code智能体卡片".into(),
            conversation: String::new(),
            cancellation: Default::default(),
        })
        .await
        .expect("官方接口应接受实际工具定义并完成模型回合");
    assert_eq!(result.tool_calls, 2, "官方模型应恰好请求两项创建");
    let calls = host.0.lock().expect("隔离宿主命令");
    assert_eq!(calls.len(), 2, "不能混入删除、重复创建或其他副作用");
    for profile in [AgentProfile::Codex, AgentProfile::Claude] {
        assert!(
            calls.contains(&AdapterCommand::CreateAgent {
                profile,
                task: None
            }),
            "两类Agent应各创建一次，合法的先后顺序不影响结果：{calls:?}"
        );
    }
}
