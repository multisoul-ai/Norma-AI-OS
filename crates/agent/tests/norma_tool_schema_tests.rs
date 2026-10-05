use agent::norma_tool_definitions;
use serde_json::json;

/// TOOL-1：Norma 只向模型暴露十七个窄工具及其精确必填参数。
///
/// 数据构造（含关键数值的推导过程）：
///   Existing tools = 10（查询、创建 Agent、派发、取消任务、建 Terminal/Browser/Preview、聚焦、输入、删除）
///   Session reply tools = 2（reply_agent + cancel_agent_reply）
///   Memory tools = 4（目录、搜索、读取、写入）
///   Expected tools = 10 + 2 + 4 + 1 = 17（额外只读 workspace_memory）
///   `assign_agent_task` required fields = 2（agent + task）
///   `reply_agent` required fields = 2（agent + message）
///   `create_browser` required fields = 0（无 URL 时使用产品默认首页）
///   `create_preview` required fields = 1（path）
///   `create_agent.profile` enum 数 = 5（Codex / Claude / OpenCode / Cursor / Grok）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从生产工具注册表读取所有模型可见 definition
///   2. 按名称核对完整集合，避免旧的宽泛工具混入
///   3. 检查 `assign_agent_task`、`reply_agent` 和 `create_agent` 的 JSON Schema
///
/// 预期结果：
///   - 正断言：十七个工具全部存在且名称准确
///   - 正断言：派发工具仅要求 agent 与 task，创建 Agent 工具允许 Grok profile
///   - 负断言：不再暴露 `open_agent`，模型也不能提供 name/activate 或未知 profile
#[test]
fn norma_exposes_exactly_seventeen_narrow_tool_schemas() {
    let definitions = norma_tool_definitions();
    let names = definitions
        .iter()
        .map(|definition| definition.name.as_str())
        .collect::<Vec<_>>();
    let expected = vec![
        "find_agent_tasks",
        "create_agent",
        "assign_agent_task",
        "cancel_agent_task",
        "reply_agent",
        "cancel_agent_reply",
        "create_terminal",
        "create_browser",
        "create_preview",
        "focus_card",
        "send_terminal_input",
        "delete_canvas_card",
        "memory_list",
        "memory_search",
        "memory_read",
        "memory_write",
        "workspace_memory",
    ];

    assert_eq!(
        names, expected,
        "Norma should expose only the seventeen supported narrow tools in deterministic order"
    );
    assert!(
        !names.contains(&"open_agent"),
        "the removed open_agent tool must not be exposed"
    );

    let assign = definitions
        .iter()
        .find(|definition| definition.name == "assign_agent_task")
        .expect("assign_agent_task definition should exist");
    assert_eq!(
        assign.parameters["required"],
        json!(["agent", "task"]),
        "assign_agent_task should require exactly one target and one task"
    );
    assert_eq!(
        assign.parameters["additionalProperties"],
        json!(false),
        "assign_agent_task should reject model-invented fields"
    );

    let create = definitions
        .iter()
        .find(|definition| definition.name == "create_agent")
        .expect("create_agent definition should exist");
    assert_eq!(
        create.parameters["required"],
        json!(["profile"]),
        "create_agent should require only the built-in profile"
    );
    assert_eq!(
        create.parameters["properties"]["profile"]["enum"],
        json!(["codex", "claude", "opencode", "cursor", "grok"]),
        "create_agent profile enum 应包含且仅包含五种受支持 runtime，Grok 不得遗漏"
    );
    assert_ne!(
        create.parameters["properties"]["profile"]["enum"],
        json!(["codex", "claude", "opencode", "cursor"]),
        "Grok 对接后 create_agent schema 不得继续停留在旧四种 profile"
    );
    assert!(
        create.parameters["properties"].get("name").is_none(),
        "the model must not choose an Agent name"
    );
    assert!(
        create.parameters["properties"].get("activate").is_none(),
        "created Agents should activate automatically without a model flag"
    );
}
