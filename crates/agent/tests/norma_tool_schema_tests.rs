use agent::norma_tool_definitions;
use serde_json::json;

/// 场景：桌面工具描述必须与右侧标签和旧状态的真实产品语义一致。
/// 数据构造：读取模型实际接收的 Browser、Preview 与工作空间快照定义。
/// 执行过程：核对每个工具的中文描述，不调用模型。
/// 预期结果：内容工具明确右侧标签；快照明确旧状态不可当作当前可审批请求。
#[test]
fn desktop_tool_descriptions_match_drawer_and_cached_status_contract() {
    let definitions = norma_tool_definitions();
    for name in ["create_browser", "create_preview"] {
        let tool = definitions
            .iter()
            .find(|tool| tool.name == name)
            .expect("桌面内容工具应注册");
        assert!(
            tool.description.contains("右侧标签"),
            "{name} 应告知模型桌面内容呈现在右侧标签"
        );
    }
    let snapshot = definitions
        .iter()
        .find(|tool| tool.name == "workspace_snapshot")
        .expect("只读快照工具应注册");
    assert!(
        snapshot.description.contains("上次状态") && snapshot.description.contains("审批"),
        "快照描述应区分未加载项目的上次状态与当前可操作审批"
    );
}

/// TOOL-1：Norma 只向模型暴露二十一个窄工具及其精确必填参数。
///
/// 数据构造（含关键数值的推导过程）：
///   Existing tools = 10（查询、创建 Agent、派发、取消任务、建 Terminal/Browser/Preview、聚焦、输入、删除）
///   Session reply tools = 2（reply_agent + cancel_agent_reply）
///   Memory tools = 4（目录、搜索、读取、写入）
///   Expected tools = 10 + 2 + 4 + 1 + 1 + 1 + 1 + 1 = 21（工作空间记忆、快照、稳定目标派活、成果打开、Agent 定位）
///   `assign_agent_task` required fields = 2（agent + task）
///   `reply_agent` required fields = 2（agent + message）
///   `create_browser` required fields = 0（无 URL 时使用产品默认首页）
///   `create_preview` required fields = 1（path）
///   `create_agent.profile` enum 数 = 9（Codex / Claude / OpenCode / Cursor / Grok / Kimi / ZCode / Pi / Hermes）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从生产工具注册表读取所有模型可见 definition
///   2. 按名称核对完整集合，避免旧的宽泛工具混入
///   3. 检查 `assign_agent_task`、`reply_agent` 和 `create_agent` 的 JSON Schema
///
/// 预期结果：
///   - 正断言：二十一个工具全部存在且名称准确
///   - 正断言：派发工具仅要求 agent 与 task，创建 Agent 工具允许 Grok profile
///   - 负断言：不再暴露 `open_agent`，模型也不能提供 name/activate 或未知 profile
#[test]
fn norma_exposes_exactly_twenty_one_narrow_tool_schemas() {
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
        "workspace_snapshot",
        "assign_workspace_task",
        "open_workspace_artifact",
        "locate_workspace_agent",
    ];

    assert_eq!(
        names, expected,
        "Norma should expose only the twenty-one supported narrow tools in deterministic order"
    );
    assert!(
        !names.contains(&"open_agent"),
        "the removed open_agent tool must not be exposed"
    );

    let snapshot = definitions
        .iter()
        .find(|definition| definition.name == "workspace_snapshot")
        .expect("workspace_snapshot 只读工具应暴露给模型");
    assert_eq!(
        snapshot.parameters["properties"]["scope"]["enum"],
        json!(["current_project", "workspace", "area"]),
        "快照应明确区分当前项目、整个工作空间和指定项目"
    );
    assert_eq!(
        snapshot.parameters["required"],
        json!(["scope"]),
        "模型必须显式选择查询范围"
    );
    let dispatch = definitions
        .iter()
        .find(|definition| definition.name == "assign_workspace_task")
        .expect("跨项目派活工具应暴露给桌面 Norma");
    assert_eq!(
        dispatch.parameters["required"],
        json!(["workspace_id", "area_id", "node_id", "task"]),
        "派活必须绑定工作空间、项目和 Agent 卡片稳定 ID"
    );
    let artifact = definitions
        .iter()
        .find(|definition| definition.name == "open_workspace_artifact")
        .expect("成果打开工具应暴露给桌面 Norma");
    assert_eq!(
        artifact.parameters["required"],
        json!([
            "workspace_id",
            "area_id",
            "node_id",
            "turn_id",
            "source_path",
            "version"
        ]),
        "成果打开必须绑定任务来源及冻结版本"
    );
    let locate = definitions
        .iter()
        .find(|definition| definition.name == "locate_workspace_agent")
        .expect("Agent 定位工具应暴露给桌面 Norma");
    assert_eq!(
        locate.parameters["required"],
        json!(["workspace_id", "area_id", "node_id"]),
        "普通 Agent 定位应仅需要三层稳定身份"
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
        json!(["codex", "claude", "opencode", "cursor", "grok", "kimi", "zcode", "pi", "hermes"]),
        "create_agent profile enum 应包含且仅包含九种受支持 runtime，Kimi/ZCode/Pi/Hermes 不得遗漏"
    );
    assert_ne!(
        create.parameters["properties"]["profile"]["enum"],
        json!(["codex", "claude", "opencode", "cursor", "grok"]),
        "新增四种 Agent 后 create_agent schema 不得继续停留在旧五种 profile"
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
