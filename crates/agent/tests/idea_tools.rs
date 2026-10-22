use agent::{
    norma_system_prompt, norma_tool_definitions, AdapterCommand, AdapterError, AdapterResult,
    AgentCardAdapter, AgentInputSource, CurrentCanvasContext, IdeaTarget, NormaToolRuntime,
    ToolContext,
};
use futures::future::BoxFuture;
use norma::protocol::AgentProfile;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct RecordingAdapter {
    commands: Mutex<Vec<AdapterCommand>>,
}

impl AgentCardAdapter for RecordingAdapter {
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        _context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            self.commands
                .lock()
                .expect("命令记录锁不应中毒")
                .push(command);
            Ok(AdapterResult::accepted("ok"))
        })
    }
}

fn context(source: AgentInputSource, turn: &str) -> ToolContext {
    ToolContext {
        project_id: "project-1".into(),
        turn_id: turn.into(),
        user_input: "记一下：给设置页加深色主题".into(),
        input_source: source,
        contextual_delete_targets: Vec::new(),
        tool_call_id: "call-1".into(),
        operation_id: Some(format!("{turn}:call-1")),
        cancellation: CancellationToken::new(),
    }
}

async fn run(
    tool: &str,
    args: Value,
) -> (Result<AdapterResult, AdapterError>, Vec<AdapterCommand>) {
    let adapter = Arc::new(RecordingAdapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());
    let result = runtime
        .execute(tool, args, &context(AgentInputSource::Other, "turn-1"))
        .await;
    let commands = adapter.commands.lock().expect("命令记录锁不应中毒").clone();
    (result, commands)
}

/// capture_idea 把正文交给适配器；空白正文与模型臆造字段在到达适配器之前被拒绝。
///
/// 数据构造：带空白的正文；纯空白；多余字段 priority。
/// 预期结果：
///   - 正断言：适配器收到 trim 后的正文。
///   - 负断言：空白、多余字段都报 InvalidArguments，且适配器没有收到任何命令。
#[tokio::test]
async fn capture_idea_trims_body_and_rejects_empty_or_unknown_fields() {
    let (result, commands) = run("capture_idea", json!({"body": "  给设置页加深色主题 "})).await;
    result.expect("正常正文应被接受");
    assert_eq!(
        commands,
        [AdapterCommand::CaptureIdea {
            body: "给设置页加深色主题".into()
        }],
        "适配器应收到 trim 后的正文"
    );
    for bad in [json!({"body": "   "}), json!({"body": "x", "priority": 1})] {
        let (result, commands) = run("capture_idea", bad.clone()).await;
        assert!(
            matches!(result, Err(AdapterError::InvalidArguments(_))),
            "{bad} 应被拒绝为参数错误"
        );
        assert!(commands.is_empty(), "非法参数不应到达适配器：{bad}");
    }
}

/// assign_idea 恰好需要一种目标：已有智能体（三元定位）或新建智能体（项目区域 + 类型）。
///
/// 数据构造：合法的两种目标；缺 node_id；两种目标混用；node_id 为 0；未知类型；缺 idea_id。
/// 预期结果：
///   - 正断言：两种合法目标映射为对应的 IdeaTarget。
///   - 负断言：其余五种都报 InvalidArguments，适配器无调用。
#[tokio::test]
async fn assign_idea_requires_exactly_one_well_formed_target() {
    let existing = json!({"idea_id":"i1","workspace_id":"w1","area_id":"a1","node_id":7});
    let (result, commands) = run("assign_idea", existing).await;
    result.expect("已有智能体目标应被接受");
    assert_eq!(
        commands,
        [AdapterCommand::AssignIdea {
            idea_id: "i1".into(),
            target: IdeaTarget::Agent {
                workspace_id: "w1".into(),
                area_id: "a1".into(),
                node_id: 7
            },
        }],
        "三元定位应映射为 IdeaTarget::Agent"
    );
    let fresh =
        json!({"idea_id":"i1","workspace_id":"w1","area_id":"a1","new_agent_profile":"codex"});
    let (result, commands) = run("assign_idea", fresh).await;
    result.expect("新建智能体目标应被接受");
    assert_eq!(
        commands,
        [AdapterCommand::AssignIdea {
            idea_id: "i1".into(),
            target: IdeaTarget::NewAgent {
                workspace_id: "w1".into(),
                area_id: "a1".into(),
                profile: AgentProfile::Codex
            },
        }],
        "新建目标应映射为 IdeaTarget::NewAgent"
    );
    let bad_cases = [
        json!({"idea_id":"i1","workspace_id":"w1","area_id":"a1"}),
        json!({"idea_id":"i1","workspace_id":"w1","area_id":"a1","node_id":7,"new_agent_profile":"codex"}),
        json!({"idea_id":"i1","workspace_id":"w1","area_id":"a1","node_id":0}),
        json!({"idea_id":"i1","workspace_id":"w1","area_id":"a1","new_agent_profile":"nope"}),
        json!({"workspace_id":"w1","area_id":"a1","node_id":7}),
    ];
    for bad in bad_cases {
        let (result, commands) = run("assign_idea", bad.clone()).await;
        assert!(
            matches!(result, Err(AdapterError::InvalidArguments(_))),
            "{bad} 应被拒绝为参数错误"
        );
        assert!(commands.is_empty(), "非法参数不应到达适配器：{bad}");
    }
}

/// 群聊讨论来源只能读，不能记录或派发想法。
///
/// 数据构造：来源为 MobileDiscussion 的上下文，依次调用 list_ideas、capture_idea、assign_idea。
/// 预期结果：
///   - 正断言：list_ideas 到达适配器。
///   - 负断言：capture_idea 与 assign_idea 被 Rejected，适配器只收到 list_ideas 一条命令。
#[tokio::test]
async fn mobile_discussion_may_list_but_not_capture_or_assign_ideas() {
    let adapter = Arc::new(RecordingAdapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());
    let ctx = context(AgentInputSource::MobileDiscussion, "turn-m");
    runtime
        .execute("list_ideas", json!({}), &ctx)
        .await
        .expect("讨论来源应可读取想法清单");
    for (tool, args) in [
        ("capture_idea", json!({"body": "x"})),
        (
            "assign_idea",
            json!({"idea_id":"i1","workspace_id":"w1","area_id":"a1","node_id":7}),
        ),
    ] {
        let result = runtime.execute(tool, args, &ctx).await;
        assert!(
            matches!(result, Err(AdapterError::Rejected(_))),
            "讨论来源不应执行 {tool}"
        );
    }
    assert_eq!(
        adapter.commands.lock().expect("锁").as_slice(),
        [AdapterCommand::ListIdeas],
        "只有只读的 list_ideas 到达适配器"
    );
}

/// 同一 operation_id 重放不重复派发；同 ID 但请求不同被拒绝。
///
/// 数据构造：同一 turn/call 连续两次相同 assign_idea；再用同 operation_id 换一个目标节点。
/// 预期结果：
///   - 正断言：第二次返回第一次的回执，适配器只执行一次。
///   - 负断言：请求内容变化时返回 Rejected，适配器仍然只有一次调用。
#[tokio::test]
async fn assign_idea_is_idempotent_per_operation_id() {
    let adapter = Arc::new(RecordingAdapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());
    let ctx = context(AgentInputSource::Other, "turn-i");
    let args = json!({"idea_id":"i1","workspace_id":"w1","area_id":"a1","node_id":7});
    runtime
        .execute("assign_idea", args.clone(), &ctx)
        .await
        .expect("首次派发应成功");
    runtime
        .execute("assign_idea", args, &ctx)
        .await
        .expect("重放应返回原回执");
    assert_eq!(
        adapter.commands.lock().expect("锁").len(),
        1,
        "重放不应再次到达适配器"
    );
    let changed = json!({"idea_id":"i1","workspace_id":"w1","area_id":"a1","node_id":8});
    let result = runtime.execute("assign_idea", changed, &ctx).await;
    assert!(
        matches!(result, Err(AdapterError::Rejected(_))),
        "同 operation_id 的不同请求应被拒绝"
    );
    assert_eq!(
        adapter.commands.lock().expect("锁").len(),
        1,
        "被拒绝的请求不应到达适配器"
    );
}

/// 三个工具的 schema 必须窄、且可被 OpenAI 兼容端接受（根上不能有 oneOf）；提示词要教模型何时记下想法。
///
/// 预期结果：
///   - 正断言：三个定义都存在、additionalProperties=false；提示词提到 capture_idea 与 assign_idea。
///   - 负断言：assign_idea 的 schema 根上没有 oneOf/anyOf。
#[test]
fn idea_tool_schemas_are_narrow_and_prompt_teaches_capture() {
    let definitions = norma_tool_definitions();
    for name in ["capture_idea", "list_ideas", "assign_idea"] {
        let definition = definitions
            .iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("{name} 定义应存在"));
        assert_eq!(
            definition.parameters["additionalProperties"],
            json!(false),
            "{name} 应拒绝臆造字段"
        );
    }
    let assign = definitions
        .iter()
        .find(|d| d.name == "assign_idea")
        .expect("assign_idea 定义应存在");
    assert!(
        assign.parameters.get("oneOf").is_none() && assign.parameters.get("anyOf").is_none(),
        "OpenAI 拒绝根上的 oneOf/anyOf，目标互斥由解析阶段保证"
    );
    let prompt = norma_system_prompt(
        &CurrentCanvasContext {
            project_name: "p".into(),
            workspace_root: "/w".into(),
            total_cards: 0,
            cards: Vec::new(),
            conversation_tails: Vec::new(),
        },
        &[],
    );
    assert!(
        prompt.contains("`capture_idea`") && prompt.contains("`assign_idea`"),
        "提示词应说明何时使用想法工具"
    );
}
