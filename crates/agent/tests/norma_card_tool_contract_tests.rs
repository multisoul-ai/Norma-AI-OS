use agent::{
    norma_system_prompt, norma_tool_definitions, AdapterCommand, AdapterError, AdapterResult,
    AgentCardAdapter, CurrentCanvasContext, NormaToolRuntime, ToolContext,
};
use futures::future::BoxFuture;
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct RecordingAdapter {
    commands: Mutex<Vec<AdapterCommand>>,
}

/// 普通文件预览已不在模型目录；有任务来源的成果仍走 file_open。
///
/// 数据构造：空画布、空名册，以及生产工具目录。
/// 执行过程：检查系统提示和工具名。
/// 预期结果：create_preview 不存在；file_open 仍说明在右侧打开；提示词不罗列卡片。
#[test]
fn norma_prompt_keeps_task_artifact_open_without_preview_tool() {
    let prompt = norma_system_prompt(
        &CurrentCanvasContext {
            project_name: "project-1".to_string(),
            workspace_root: "/workspace/project-1".to_string(),
            total_cards: 0,
            cards: Vec::new(),
            conversation_tails: Vec::new(),
        },
        &[],
    );
    let definitions = norma_tool_definitions();
    assert!(
        definitions.iter().all(|tool| tool.name != "create_preview"),
        "模型不能再创建 Preview 卡片"
    );
    let file_open = definitions
        .iter()
        .find(|tool| tool.name == "file_open")
        .expect("有任务来源的成果仍由 file_open 打开");
    assert!(
        file_open.description.contains("右侧"),
        "成果仍在右侧打开：{}",
        file_open.description
    );
    assert!(
        !prompt.contains("当前画布卡片"),
        "精简后的系统提示不再罗列卡片：{prompt}"
    );
}

/// Norma 路由应把“创建 Agent 并执行任务”合并为一次 agent_create 调用。
///
/// 数据构造（含关键数值的推导过程）：
///   User intents = 2（创建卡片 + 执行任务）
///   Required model tool calls = 1（`agent_create` with `task`）
///   Forbidden follow-up calls = 1（额外 `agent_assign`）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用空画布与空 Agent 名册构建生产 system prompt。
///   2. 检查新建且带任务的明确路由规则。
///   3. 检查该规则是否禁止紧接着再次派发同一任务。
///
/// 预期结果：
///   - 正断言：提示词要求把完整任务放进 `agent_create.task`。
///   - 正断言：提示词明确限定整个新建场景只调用一次工具。
///   - 负断言：提示词不得允许随后再调用 `agent_assign` 造成串行往返或重复任务。
#[test]
fn norma_prompt_routes_create_and_task_to_one_tool_call() {
    let prompt = norma_system_prompt(
        &CurrentCanvasContext {
            project_name: "project-1".to_string(),
            workspace_root: "/workspace/project-1".to_string(),
            total_cards: 0,
            cards: Vec::new(),
            conversation_tails: Vec::new(),
        },
        &[],
    );

    assert!(
        prompt.contains("agent_create")
            && prompt.contains("只说「创建」时省略 task")
            && prompt.contains("才把任务放进 task，且不要再 assign"),
        "只创建时省略 task；新建并做事时任务放进 agent_create.task，不再额外 assign：{prompt}"
    );
    assert!(
        !prompt.contains("先调用 `agent_create`，再调用 `agent_assign`"),
        "Norma prompt must not preserve the old two-call create-then-assign route"
    );
}

/// Norma 创建 Agent 时携带任务，应通过一次 typed tool 调用抵达 Adapter。
///
/// 数据构造（含关键数值的推导过程）：
///   Tool calls = 1（`agent_create`）
///   Requested Agent profiles = 1（Codex）
///   Attached tasks = 1（检查技术栈）
///   Expected Adapter commands = 1 + 0 additional assignment calls = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 检查生产 schema 将 `task` 声明为可选非空字符串。
///   2. 使用 `profile=codex` 与非空 `task` 调用生产 `NormaToolRuntime`。
///   3. Runtime 合并参数为一个命令，RecordingAdapter 记录并返回 accepted 回执。
///
/// 预期结果：
///   - 正断言：schema 暴露可选 task，同时仍只强制要求 profile。
///   - 正断言：工具调用成功并保留 Adapter 的 accepted 状态。
///   - 正断言：Adapter 恰好只执行一次，避免第二次串行工具调用。
///   - 负断言：可选 task 不得被 `deny_unknown_fields` 拒绝或丢弃。
#[tokio::test]
async fn agent_create_accepts_attached_task_in_one_adapter_call() {
    let adapter = Arc::new(RecordingAdapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());
    let create = norma_tool_definitions()
        .into_iter()
        .find(|definition| definition.name == "agent_create")
        .expect("agent_create definition should exist");
    let context = ToolContext {
        project_id: "project-1".to_string(),
        turn_id: "turn-create-and-dispatch".to_string(),
        user_input: "创建一个 Codex，并让它检查当前项目有哪些技术栈。".to_string(),
        input_source: agent::AgentInputSource::Other,
        voice_commit: None,
        contextual_delete_targets: Vec::new(),
        tool_call_id: "call-1".to_string(),
        operation_id: Some("turn-create-and-dispatch:call-1".to_string()),
        cancellation: CancellationToken::new(),
    };

    let result = runtime
        .execute(
            "agent_create",
            json!({
                "profile": "codex",
                "task": "检查当前项目有哪些技术栈。"
            }),
            &context,
        )
        .await
        .expect("agent_create should accept one optional task without a second tool call");

    assert_eq!(
        create.parameters["properties"]["task"],
        json!({"type": "string", "minLength": 1}),
        "agent_create schema should expose one optional non-empty task"
    );
    assert_eq!(
        create.parameters["required"],
        json!(["profile"]),
        "task should stay optional so card-only agent_create calls remain compatible"
    );
    assert_eq!(
        result.status, "accepted",
        "agent_create with a task should preserve the Adapter's accepted receipt"
    );
    let commands = adapter
        .commands
        .lock()
        .expect("recording adapter command lock should not be poisoned");
    assert_eq!(
        commands.len(),
        1,
        "agent_create with a task should execute exactly one Adapter command"
    );
    assert_eq!(
        commands[0],
        AdapterCommand::CreateAgent {
            profile: norma::protocol::AgentProfile::Codex,
            task: Some("检查当前项目有哪些技术栈。".to_string()),
        },
        "agent_create should preserve the requested profile and complete task in one command"
    );
    assert!(
        !matches!(commands[0], AdapterCommand::AssignAgentTask { .. }),
        "the combined create command must not degrade into a separate assignment command"
    );
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
                .expect("recording adapter command lock should not be poisoned")
                .push(command);
            Ok(AdapterResult::accepted("card accepted"))
        })
    }
}

/// 已从模型目录删除的工具名必须报参数错误，不能静默执行菜单命令。
///
/// 数据构造：四个旧名字，各带一份以前能通过的参数。
/// 执行过程：经 NormaToolRuntime 解析。
/// 预期结果：全部 InvalidArguments，适配器零调用。
#[tokio::test]
async fn removed_model_tool_names_do_not_execute() {
    let adapter = Arc::new(RecordingAdapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());
    let context = ToolContext {
        project_id: "project-1".to_string(),
        turn_id: "turn-removed".to_string(),
        user_input: "打开网页".to_string(),
        input_source: agent::AgentInputSource::Other,
        voice_commit: None,
        contextual_delete_targets: Vec::new(),
        tool_call_id: "call-removed".to_string(),
        operation_id: Some("turn-removed:call-removed".to_string()),
        cancellation: CancellationToken::new(),
    };
    for (name, arguments) in [
        (
            "create_browser",
            json!({"url": "https://docs.example.com/guide"}),
        ),
        (
            "create_preview",
            json!({"path": "docs/report.md", "preview_type": "markdown"}),
        ),
        ("focus_card", json!({"node_id": 1})),
        (
            "send_terminal_input",
            json!({"node_id": 1, "text": "git status", "submit": true}),
        ),
        (
            "assign_idea",
            json!({"idea_id": "i1", "workspace_id": "w", "area_id": "a", "node_id": 1}),
        ),
    ] {
        let result = runtime.execute(name, arguments, &context).await;
        assert!(
            matches!(result, Err(AdapterError::InvalidArguments(_))),
            "{name} 应作为未知工具被拒绝：{result:?}"
        );
    }
    assert!(
        adapter
            .commands
            .lock()
            .expect("recording adapter command lock should not be poisoned")
            .is_empty(),
        "旧工具名不得落到菜单里的 Browser、Preview 或 AssignIdea"
    );
}
