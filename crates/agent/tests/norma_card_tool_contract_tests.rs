use agent::{
    norma_system_prompt, norma_tool_definitions, AdapterCommand, AdapterError, AdapterResult,
    AgentCardAdapter, CurrentCanvasContext, NormaToolRuntime, ToolContext,
};
use futures::future::BoxFuture;
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct RecordingAdapter {
    commands: Mutex<Vec<AdapterCommand>>,
}

/// Norma 提示词应把用户用语 REVIEW 明确路由到现有文件 Preview 工具。
///
/// 数据构造（含关键数值的推导过程）：
///   Canvas cards = 0
///   Agent roster entries = 0
///   Review route = 1（REVIEW → create_preview）
///   Missing-path clarification limit = 1 次
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用空画布构建生产 system prompt。
///   2. 检查 REVIEW 别称、目标工具与缺少路径时的行为约束。
///
/// 预期结果：
///   - 正断言：REVIEW 必须明确映射到 `create_preview`。
///   - 正断言：缺少路径时只允许一次最小澄清。
///   - 负断言：REVIEW 不得被路由到 Terminal 工具或允许猜测文件。
#[test]
fn norma_prompt_routes_review_card_language_to_file_preview_tool() {
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
        prompt.contains("REVIEW 卡片按现有产品语义指文件 Preview 卡片")
            && prompt.contains("调用 `create_preview`"),
        "Norma prompt should deterministically route REVIEW card wording to create_preview"
    );
    assert!(
        prompt.contains("只询问一次路径"),
        "Norma prompt should require one minimal clarification when a REVIEW path is missing"
    );
    assert!(
        !prompt.contains("REVIEW 卡片时调用 `create_terminal`") && prompt.contains("不要猜测"),
        "Norma must not route REVIEW to Terminal or invent a target file"
    );
}

/// Norma 路由应把“创建 Agent 并执行任务”合并为一次 create_agent 调用。
///
/// 数据构造（含关键数值的推导过程）：
///   User intents = 2（创建卡片 + 执行任务）
///   Required model tool calls = 1（`create_agent` with `task`）
///   Forbidden follow-up calls = 1（额外 `assign_agent_task`）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用空画布与空 Agent 名册构建生产 system prompt。
///   2. 检查新建且带任务的明确路由规则。
///   3. 检查该规则是否禁止紧接着再次派发同一任务。
///
/// 预期结果：
///   - 正断言：提示词要求把完整任务放进 `create_agent.task`。
///   - 正断言：提示词明确限定整个新建场景只调用一次工具。
///   - 负断言：提示词不得允许随后再调用 `assign_agent_task` 造成串行往返或重复任务。
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
        prompt.contains("把完整任务放入 `create_agent.task`"),
        "Norma prompt should route the complete new-Agent task into create_agent.task"
    );
    assert!(
        prompt.contains("只调用一次 `create_agent`")
            && prompt.contains("不要随后再调用 `assign_agent_task`"),
        "Norma prompt should require one combined tool call and forbid a serial follow-up assignment"
    );
    assert!(
        !prompt.contains("先调用 `create_agent`，再调用 `assign_agent_task`"),
        "Norma prompt must not preserve the old two-call create-then-assign route"
    );
}

/// Norma 创建 Agent 时携带任务，应通过一次 typed tool 调用抵达 Adapter。
///
/// 数据构造（含关键数值的推导过程）：
///   Tool calls = 1（`create_agent`）
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
async fn create_agent_accepts_attached_task_in_one_adapter_call() {
    let adapter = Arc::new(RecordingAdapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());
    let create = norma_tool_definitions()
        .into_iter()
        .find(|definition| definition.name == "create_agent")
        .expect("create_agent definition should exist");
    let context = ToolContext {
        project_id: "project-1".to_string(),
        turn_id: "turn-create-and-dispatch".to_string(),
        user_input: "创建一个 Codex，并让它检查当前项目有哪些技术栈。".to_string(),
        input_source: agent::AgentInputSource::Other,
        contextual_delete_targets: Vec::new(),
        tool_call_id: "call-1".to_string(),
        operation_id: Some("turn-create-and-dispatch:call-1".to_string()),
        cancellation: CancellationToken::new(),
    };

    let result = runtime
        .execute(
            "create_agent",
            json!({
                "profile": "codex",
                "task": "检查当前项目有哪些技术栈。"
            }),
            &context,
        )
        .await
        .expect("create_agent should accept one optional task without a second tool call");

    assert_eq!(
        create.parameters["properties"]["task"],
        json!({"type": "string", "minLength": 1}),
        "create_agent schema should expose one optional non-empty task"
    );
    assert_eq!(
        create.parameters["required"],
        json!(["profile"]),
        "task should stay optional so card-only create_agent calls remain compatible"
    );
    assert_eq!(
        result.status, "accepted",
        "create_agent with a task should preserve the Adapter's accepted receipt"
    );
    let commands = adapter
        .commands
        .lock()
        .expect("recording adapter command lock should not be poisoned");
    assert_eq!(
        commands.len(),
        1,
        "create_agent with a task should execute exactly one Adapter command"
    );
    assert_eq!(
        commands[0],
        AdapterCommand::CreateAgent {
            profile: norma::protocol::AgentProfile::Codex,
            task: Some("检查当前项目有哪些技术栈。".to_string()),
        },
        "create_agent should preserve the requested profile and complete task in one command"
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

/// Browser 与 Preview 工具 schema 应保持窄参数边界。
///
/// 数据构造（含关键数值的推导过程）：
///   New card tools = 2（create_browser + create_preview）
///   Browser required fields = 0（允许使用默认首页）
///   Preview required fields = 1（path）
///   Preview type values = 3（markdown + text + image）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从生产工具注册表定位 Browser 与 Preview definition。
///   2. 检查 required、additionalProperties 与 preview_type 枚举。
///
/// 预期结果：
///   - 正断言：Browser 可省略 URL，Preview 只强制要求 path。
///   - 正断言：Preview 类型只允许三个既有 renderer。
///   - 负断言：两个 schema 都不得接收模型臆造字段。
#[test]
fn browser_and_preview_tool_schemas_keep_narrow_card_arguments() {
    let definitions = norma_tool_definitions();
    let browser = definitions
        .iter()
        .find(|definition| definition.name == "create_browser")
        .expect("create_browser definition should exist");
    let preview = definitions
        .iter()
        .find(|definition| definition.name == "create_preview")
        .expect("create_preview definition should exist");

    assert_eq!(
        browser.parameters["required"],
        json!([]),
        "create_browser should allow the product default page when URL is omitted"
    );
    assert_eq!(
        browser.parameters["additionalProperties"],
        json!(false),
        "create_browser should reject model-invented fields"
    );
    assert_eq!(
        preview.parameters["required"],
        json!(["path"]),
        "create_preview should require exactly the file path"
    );
    assert_eq!(
        preview.parameters["properties"]["preview_type"]["enum"],
        json!(["markdown", "text", "image"]),
        "create_preview should constrain explicit renderer types"
    );
    assert_eq!(
        preview.parameters["additionalProperties"],
        json!(false),
        "create_preview should reject model-invented fields"
    );
}

/// Browser 与 Preview typed tools 应把结构化参数原样交给 Adapter。
///
/// 数据构造（含关键数值的推导过程）：
///   Tool calls = 2（create_browser + create_preview）
///   Browser URL = 1 个完整 HTTPS URL
///   Preview path = 1 个工作区相对 Markdown 路径
///   Expected Adapter commands = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用 `create_browser`，传入文档页 URL。
///   2. 调用 `create_preview`，传入报告路径与 markdown 类型。
///   3. 从 RecordingAdapter 读取按调用顺序收到的命令。
///
/// 预期结果：
///   - 正断言：Browser URL 与 Preview 路径/类型完整保留。
///   - 正断言：两个工具均返回 Adapter 的真实 accepted 回执。
///   - 负断言：两个创建请求都不得退化成 CreateTerminal。
#[tokio::test]
async fn browser_and_preview_tools_preserve_structured_card_payloads() {
    let adapter = Arc::new(RecordingAdapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());
    let context = |turn: &str, call: &str, input: &str| ToolContext {
        project_id: "project-1".to_string(),
        turn_id: turn.to_string(),
        user_input: input.to_string(),
        input_source: agent::AgentInputSource::Other,
        contextual_delete_targets: Vec::new(),
        tool_call_id: call.to_string(),
        operation_id: Some(format!("{turn}:{call}")),
        cancellation: CancellationToken::new(),
    };

    let browser = runtime
        .execute(
            "create_browser",
            json!({"url": "https://docs.example.com/guide"}),
            &context("turn-browser", "call-1", "打开文档网页"),
        )
        .await
        .expect("create_browser should reach the Adapter");
    let preview = runtime
        .execute(
            "create_preview",
            json!({"path": "docs/report.md", "preview_type": "markdown"}),
            &context("turn-preview", "call-1", "打开报告预览"),
        )
        .await
        .expect("create_preview should reach the Adapter");

    assert_eq!(
        browser.status, "accepted",
        "create_browser should return the Adapter's actual accepted status"
    );
    assert_eq!(
        preview.status, "accepted",
        "create_preview should return the Adapter's actual accepted status"
    );
    let commands = adapter
        .commands
        .lock()
        .expect("recording adapter command lock should not be poisoned");
    assert_eq!(
        commands.len(),
        2,
        "the two card tools should produce exactly two Adapter commands"
    );
    assert_eq!(
        commands[0],
        AdapterCommand::CreateBrowser {
            url: Some("https://docs.example.com/guide".to_string()),
        },
        "create_browser should preserve the exact requested URL"
    );
    assert_eq!(
        commands[1],
        AdapterCommand::CreatePreview {
            path: PathBuf::from("docs/report.md"),
            explicit_type: Some(norma::protocol::PreviewType::Markdown),
        },
        "create_preview should preserve path and explicit Markdown renderer"
    );
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, AdapterCommand::CreateTerminal)),
        "Browser and Preview tools must not create Terminal cards"
    );
}
