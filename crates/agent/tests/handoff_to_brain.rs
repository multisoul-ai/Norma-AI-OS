use agent::{
    norma_system_prompt, norma_tool_definitions, AdapterCommand, AdapterError, AdapterResult,
    AgentCardAdapter, AgentInputSource, CurrentCanvasContext, NormaToolRuntime, ToolContext,
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

async fn run(args: Value) -> (Result<AdapterResult, AdapterError>, Vec<AdapterCommand>) {
    let adapter = Arc::new(RecordingAdapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());
    let context = ToolContext {
        project_id: "project-1".into(),
        turn_id: "turn-1".into(),
        user_input: "让 Codex 把 ACP 文档补上".into(),
        input_source: AgentInputSource::Other,
        contextual_delete_targets: Vec::new(),
        tool_call_id: "call-1".into(),
        operation_id: Some("turn-1:call-1".into()),
        cancellation: CancellationToken::new(),
    };
    let result = runtime.execute("handoff_to_brain", args, &context).await;
    let commands = adapter.commands.lock().expect("命令记录锁不应中毒").clone();
    (result, commands)
}

/// handoff_to_brain 把请求原文交给适配器；空白请求和多余字段在到达适配器之前被拒绝。
///
/// 数据构造：带首尾空白的请求；纯空白；多余字段 priority。
/// 预期结果：
///   - 正断言：适配器收到 trim 后的请求，命令是 HandoffToBrain
///   - 负断言：空白、多余字段都报 InvalidArguments，且适配器没有收到任何命令
#[tokio::test]
async fn handoff_passes_the_request_and_rejects_bad_arguments() {
    let (result, commands) = run(json!({"request": "  让 Codex 把 ACP 文档补上 "})).await;
    result.expect("正常请求应被接受");
    assert_eq!(
        commands,
        vec![AdapterCommand::HandoffToBrain {
            request: "让 Codex 把 ACP 文档补上".into()
        }],
        "适配器应收到 trim 后的请求"
    );
    for args in [
        json!({"request": "   "}),
        json!({"request": "x", "priority": "now"}),
    ] {
        let (result, commands) = run(args.clone()).await;
        assert!(
            matches!(result, Err(AdapterError::InvalidArguments(_))),
            "{args} 应被拒绝：{result:?}"
        );
        assert!(commands.is_empty(), "非法参数不应到达适配器：{commands:?}");
    }
}

/// 工具定义与系统提示都要提到交接工具，否则模型不会使用它。
///
/// 预期结果：
///   - 正断言：定义里有 handoff_to_brain，参数只有必填的 request；系统提示提到它
///   - 负断言：定义里没有重复的工具名
#[test]
fn the_tool_is_defined_once_and_described_in_the_prompt() {
    let definitions = norma_tool_definitions();
    let matching: Vec<_> = definitions
        .iter()
        .filter(|d| d.name == "handoff_to_brain")
        .collect();
    assert_eq!(matching.len(), 1, "工具名不能重复");
    assert_eq!(
        matching[0].parameters["required"],
        json!(["request"]),
        "request 必填"
    );
    assert!(
        norma_system_prompt(
            &CurrentCanvasContext {
                project_name: "p".into(),
                workspace_root: "/w".into(),
                total_cards: 0,
                cards: Vec::new(),
                conversation_tails: Vec::new(),
            },
            &[],
        )
        .contains("handoff_to_brain"),
        "系统提示要告诉模型何时交接"
    );
}
