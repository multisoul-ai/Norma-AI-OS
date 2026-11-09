use agent::{
    norma_tool_definitions, AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter,
    AgentInputSource, NormaToolRuntime, ToolContext,
};
use futures::future::BoxFuture;
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
        voice_commit: None,
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

/// idea_save 把正文交给适配器；空白正文与模型臆造字段在到达适配器之前被拒绝。
///
/// 数据构造：带空白的正文；纯空白；多余字段 priority。
/// 预期结果：
///   - 正断言：适配器收到 trim 后的正文。
///   - 负断言：空白、多余字段都报 InvalidArguments，且适配器没有收到任何命令。
#[tokio::test]
async fn idea_save_trims_body_and_rejects_empty_or_unknown_fields() {
    let (result, commands) = run("idea_save", json!({"body": "  给设置页加深色主题 "})).await;
    result.expect("正常正文应被接受");
    assert_eq!(
        commands,
        [AdapterCommand::CaptureIdea {
            body: "给设置页加深色主题".into()
        }],
        "适配器应收到 trim 后的正文"
    );
    for bad in [json!({"body": "   "}), json!({"body": "x", "priority": 1})] {
        let (result, commands) = run("idea_save", bad.clone()).await;
        assert!(
            matches!(result, Err(AdapterError::InvalidArguments(_))),
            "{bad} 应被拒绝为参数错误"
        );
        assert!(commands.is_empty(), "非法参数不应到达适配器：{bad}");
    }
}

/// 模型不能再调用 assign_idea。拖到卡片上仍走 UI 的 AdapterCommand，不走工具名。
///
/// 数据构造：以前合法的三元定位参数。
/// 预期结果：未知工具，InvalidArguments，适配器无调用。
#[tokio::test]
async fn assign_idea_is_not_a_model_tool() {
    let (result, commands) = run(
        "assign_idea",
        json!({"idea_id":"i1","workspace_id":"w1","area_id":"a1","node_id":7}),
    )
    .await;
    assert!(
        matches!(result, Err(AdapterError::InvalidArguments(_))),
        "assign_idea 应作为未知工具被拒绝：{result:?}"
    );
    assert!(commands.is_empty(), "未知工具不应到达适配器");
}

/// 群聊讨论来源只能读，不能记录或派发想法。
///
/// 数据构造：来源为 MobileDiscussion 的上下文，依次调用 idea_list、idea_save、agent_assign。
/// 预期结果：
///   - 正断言：idea_list 到达适配器。
///   - 负断言：idea_save 与 agent_assign 被 Rejected，适配器只收到 idea_list 一条命令。
#[tokio::test]
async fn mobile_discussion_may_list_but_not_capture_or_assign_ideas() {
    let adapter = Arc::new(RecordingAdapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());
    let ctx = context(AgentInputSource::MobileDiscussion, "turn-m");
    runtime
        .execute("idea_list", json!({}), &ctx)
        .await
        .expect("讨论来源应可读取想法清单");
    for (tool, args) in [
        ("idea_save", json!({"body": "x"})),
        ("agent_assign", json!({"agent":"Daisy","task":"检查登录"})),
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
        "只有只读的 idea_list 到达适配器"
    );
}

/// 同一 operation_id 重放不重复记录；同 ID 但正文不同被拒绝。
///
/// 数据构造：同一 turn/call 连续两次相同 idea_save；再用同 operation_id 换正文。
/// 预期结果：
///   - 正断言：第二次返回第一次的回执，适配器只执行一次。
///   - 负断言：请求内容变化时返回 Rejected，适配器仍然只有一次调用。
#[tokio::test]
async fn idea_save_is_idempotent_per_operation_id() {
    let adapter = Arc::new(RecordingAdapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());
    let ctx = context(AgentInputSource::Other, "turn-i");
    let args = json!({"body":"给设置页加深色主题"});
    runtime
        .execute("idea_save", args.clone(), &ctx)
        .await
        .expect("首次记录应成功");
    runtime
        .execute("idea_save", args, &ctx)
        .await
        .expect("重放应返回原回执");
    assert_eq!(
        adapter.commands.lock().expect("锁").len(),
        1,
        "重放不应再次到达适配器"
    );
    let changed = json!({"body":"换一条想法"});
    let result = runtime.execute("idea_save", changed, &ctx).await;
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

/// 想法工具的 schema 必须窄，且根上没有 oneOf；记下想法的时机留在工具说明里。
///
/// 预期结果：idea_save 与 idea_list 存在且 additionalProperties=false；assign_idea 不在目录中。
#[test]
fn idea_tool_schemas_are_narrow_and_prompt_teaches_capture() {
    let definitions = norma_tool_definitions();
    assert!(
        definitions.iter().all(|tool| tool.name != "assign_idea"),
        "模型目录不再包含 assign_idea"
    );
    for name in ["idea_save", "idea_list"] {
        let definition = definitions
            .iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("{name} 定义应存在"));
        assert_eq!(
            definition.parameters["additionalProperties"],
            json!(false),
            "{name} 应拒绝臆造字段"
        );
        assert!(
            definition.parameters.get("oneOf").is_none()
                && definition.parameters.get("anyOf").is_none(),
            "{name} 根上不能有 oneOf/anyOf"
        );
    }
    let capture = definitions
        .iter()
        .find(|tool| tool.name == "idea_save")
        .expect("idea_save 定义应存在");
    assert!(
        capture.description.contains("记"),
        "记下想法的时机留在工具说明里"
    );
}
