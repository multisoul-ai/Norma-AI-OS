use agent::{
    AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, AgentRosterEntry,
    AgentTrackingMode, CanvasCardContext, CanvasCardKind, CurrentCanvasContext, NormaSession,
    ProviderConfig, ProviderProtocol, ToolContext,
};
use futures::future::BoxFuture;
use norma::protocol::AgentProfile;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct LiveSmokeAdapter {
    commands: Mutex<Vec<AdapterCommand>>,
    tool_call_ids: Mutex<Vec<String>>,
}

impl AgentCardAdapter for LiveSmokeAdapter {
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            let result = match &command {
                AdapterCommand::AssignAgentTask { agent, .. } => {
                    AdapterResult::accepted(format!("{agent} 的 AgentCard 已接收任务"))
                        .with_task_id(101)
                        .with_node_id(21)
                }
                AdapterCommand::CreateAgent { .. } => {
                    AdapterResult::succeeded("已创建 Agent").with_node_id(22)
                }
                AdapterCommand::CreateTerminal => {
                    AdapterResult::succeeded("已创建 Terminal").with_node_id(23)
                }
                AdapterCommand::CreateBrowser { .. } => {
                    AdapterResult::succeeded("已创建 Browser").with_node_id(24)
                }
                AdapterCommand::CreatePreview { .. } => {
                    AdapterResult::succeeded("已创建 Preview").with_node_id(25)
                }
                AdapterCommand::FocusCard { node_id }
                | AdapterCommand::SendTerminalInput { node_id, .. }
                | AdapterCommand::DeleteCanvasCard { node_id } => {
                    AdapterResult::succeeded("Canvas 操作成功").with_node_id(*node_id)
                }
                AdapterCommand::FindAgentTasks { .. } => AdapterResult::succeeded("找到 1 个任务")
                    .with_tasks(vec![serde_json::json!({
                        "task_id": 101,
                        "agent": "Alan",
                        "state": "running"
                    })]),
                AdapterCommand::CancelAgentTask { task_id } => {
                    AdapterResult::accepted("已请求取消").with_task_id(*task_id)
                }
                AdapterCommand::ReplyAgent { agent, .. } => {
                    AdapterResult::succeeded(format!("已经发给 {agent} 了。"))
                        .with_node_id(21)
                        .with_reply_id(7)
                        .with_reply_status("sent")
                }
                AdapterCommand::CancelAgentReply { .. } => {
                    AdapterResult::succeeded("还没发出的回复已经撤回。")
                        .with_reply_status("cancelled")
                }
            };
            self.commands
                .lock()
                .expect("live smoke command lock should not be poisoned")
                .push(command);
            self.tool_call_ids
                .lock()
                .expect("live smoke tool-call ID lock should not be poisoned")
                .push(context.tool_call_id.clone());
            Ok(result)
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base_url = std::env::var("NORMA_LIVE_BASE_URL")?;
    let api_key = std::env::var("NORMA_LIVE_API_KEY")?;
    let model = std::env::var("NORMA_LIVE_MODEL")?;
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        base_url,
        model,
        Some(api_key),
    );
    let adapter = Arc::new(LiveSmokeAdapter::default());
    let mut session = NormaSession::new("norma-live-smoke", &config, adapter.clone())?;
    session.update_environment(
        CurrentCanvasContext {
            project_name: "norma-ai-os".to_string(),
            workspace_root: "/workspace/norma-ai-os".to_string(),
            total_cards: 1,
            cards: vec![CanvasCardContext {
                node_id: 21,
                kind: CanvasCardKind::Agent,
                title: "Alan".to_string(),
                selected: false,
                focused: true,
            }],
            conversation_tails: Vec::new(),
        },
        vec![AgentRosterEntry {
            definition_id: 1,
            name: "Alan".to_string(),
            speech_alias: None,
            profile: AgentProfile::Codex,
            enabled: true,
            active_node_id: Some(21),
            tracking: AgentTrackingMode::Managed,
            busy: false,
        }],
    );

    let (ordinary, ordinary_chunks) =
        run_streamed_turn(&mut session, "请只用一句中文说明你是谁。不要调用工具。").await?;
    println!("ORDINARY_REPLY={}", ordinary.final_text.replace('\n', " "));
    println!("ORDINARY_TOOL_CALLS={}", ordinary.tool_calls);
    println!("ORDINARY_STREAM_CHUNKS={}", ordinary_chunks.len());

    let assignment = session
        .turn("Alan，请你检查一下当前项目有哪些技术栈。")
        .await?;
    println!("ASSIGN_REPLY={}", assignment.final_text.replace('\n', " "));
    println!("ASSIGN_TOOL_CALLS={}", assignment.tool_calls);

    let creation = session
        .turn("创建一个 Codex、一个 Claude Code、一个 OpenCode，不需要确认。")
        .await?;
    println!("CREATE_REPLY={}", creation.final_text.replace('\n', " "));
    println!("CREATE_TOOL_CALLS={}", creation.tool_calls);
    let commands = adapter
        .commands
        .lock()
        .expect("live smoke command lock should not be poisoned");
    println!("ADAPTER_COMMAND_COUNT={}", commands.len());
    println!("ADAPTER_COMMANDS={commands:?}");
    assert_eq!(ordinary.tool_calls, 0, "ordinary reply must not call tools");
    assert!(
        !ordinary_chunks.is_empty(),
        "ordinary reply must arrive through at least one streaming chunk"
    );
    assert_eq!(
        ordinary_chunks.concat(),
        ordinary.final_text,
        "streamed ordinary chunks must exactly reconstruct the final visible reply"
    );
    assert_eq!(
        assignment.tool_calls, 1,
        "direct Alan addressing must execute exactly one assignment tool"
    );
    assert_eq!(
        creation.tool_calls, 3,
        "creating three requested Agent profiles must execute exactly three tools"
    );
    assert_eq!(
        commands.len(),
        4,
        "the smoke must execute four Adapter commands"
    );
    let tool_call_ids = adapter
        .tool_call_ids
        .lock()
        .expect("live smoke tool-call ID lock should not be poisoned");
    assert_eq!(
        tool_call_ids.len(),
        4,
        "every Adapter command must receive one real Rig tool-call identity"
    );
    assert!(
        tool_call_ids.iter().all(|id| !id.starts_with("tool-")),
        "Adapter identities must come from Provider/Rig rather than local tool-N counters"
    );
    Ok(())
}

async fn run_streamed_turn(
    session: &mut NormaSession,
    input: &str,
) -> Result<(agent::NormaTurnResult, Vec<String>), agent::NormaSessionError> {
    let (deltas, mut received) = tokio::sync::mpsc::channel(32);
    let collector = tokio::spawn(async move {
        let mut chunks = Vec::new();
        while let Some(chunk) = received.recv().await {
            chunks.push(chunk);
        }
        chunks
    });
    let result = session.turn_streaming(input, deltas.clone()).await?;
    drop(deltas);
    let chunks = collector
        .await
        .expect("live smoke streaming collector should not panic");
    Ok((result, chunks))
}
