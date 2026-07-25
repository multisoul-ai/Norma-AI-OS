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
