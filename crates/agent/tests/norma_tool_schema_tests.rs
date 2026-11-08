use agent::{model_tool_visible, norma_tool_definitions, ModelToolScope};
use serde_json::json;

/// 场景：桌面工具描述必须与右侧文件标签和旧状态的真实产品语义一致。
/// 数据构造：读取模型实际接收的成果打开与工作空间快照定义。
/// 执行过程：核对中文描述，并确认已删除的网页、预览工具不在目录里。
/// 预期结果：成果打开说明右侧文件标签；快照区分上次状态与当前可审批请求。
#[test]
fn desktop_tool_descriptions_match_drawer_and_cached_status_contract() {
    let definitions = norma_tool_definitions();
    for name in [
        "create_browser",
        "create_preview",
        "focus_card",
        "send_terminal_input",
    ] {
        assert!(
            definitions.iter().all(|tool| tool.name != name),
            "{name} 已从模型目录移除"
        );
    }
    let file_open = definitions
        .iter()
        .find(|tool| tool.name == "file_open")
        .expect("成果打开工具应注册");
    assert!(
        file_open.description.contains("右侧") && file_open.description.contains("文件标签"),
        "file_open 应告知模型成果在右侧文件标签打开"
    );
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
///   Project tools = 8（task_list、agent_create、agent_assign、task_cancel、agent_reply、reply_cancel、terminal_create、card_delete）
///   Memory tools = 4
///   Desktop / ideas / brain / context = 2 + 4 + 1 + 2（workspace_memory、snapshot、file_open、agent_show、idea_save、idea_list、handoff、context 读写）
///   Expected tools = 8 + 4 + 2 + 4 + 1 + 2 = 21
///   `agent_assign` required fields = 1（只有 task；呼叫名与三元 ID 互斥由解析器保证，schema 根上不用 oneOf）
///   `agent_create.profile` enum 数 = 10
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从生产工具注册表读取所有模型可见 definition
///   2. 按名称核对完整集合，避免已删除工具混入
///   3. 检查 `agent_assign` 与 `agent_create` 的 JSON Schema
///
/// 预期结果：
///   - 正断言：二十一个工具按注册顺序存在
///   - 正断言：派发只强制 task，两种地址都是可选字段
///   - 负断言：不再暴露网页、预览、聚焦、终端输入、assign_idea 或 open_agent
#[test]
fn norma_exposes_exactly_twenty_one_narrow_tool_schemas() {
    let definitions = norma_tool_definitions();
    let names = definitions
        .iter()
        .map(|definition| definition.name.as_str())
        .collect::<Vec<_>>();
    let expected = vec![
        "task_list",
        "agent_create",
        "agent_assign",
        "task_cancel",
        "agent_reply",
        "reply_cancel",
        "terminal_create",
        "card_delete",
        "memory_list",
        "memory_search",
        "memory_read",
        "memory_write",
        "workspace_memory",
        "workspace_snapshot",
        "file_open",
        "agent_show",
        "idea_save",
        "idea_list",
        "handoff_to_brain",
        "context_read",
        "context_update",
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
        .find(|definition| definition.name == "agent_assign")
        .expect("派活工具应暴露给 Norma");
    for field in ["agent", "workspace_id", "area_id", "node_id", "task"] {
        assert!(
            dispatch.parameters["properties"].get(field).is_some(),
            "agent_assign 应同时提供呼叫名和三元 ID 字段：{field}"
        );
    }
    let artifact = definitions
        .iter()
        .find(|definition| definition.name == "file_open")
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
        .find(|definition| definition.name == "agent_show")
        .expect("Agent 定位工具应暴露给桌面 Norma");
    assert_eq!(
        locate.parameters["required"],
        json!(["workspace_id", "area_id", "node_id"]),
        "普通 Agent 定位应仅需要三层稳定身份"
    );

    let assign = definitions
        .iter()
        .find(|definition| definition.name == "agent_assign")
        .expect("agent_assign definition should exist");
    assert_eq!(
        assign.parameters["required"],
        json!(["task"]),
        "agent_assign 只强制 task；地址互斥由解析器拒绝，避免 GPT-Live 剥掉根 oneOf"
    );
    assert!(
        assign.parameters.get("oneOf").is_none(),
        "agent_assign 的 schema 根上不能有 oneOf"
    );
    assert_eq!(
        assign.parameters["additionalProperties"],
        json!(false),
        "agent_assign should reject model-invented fields"
    );

    let create = definitions
        .iter()
        .find(|definition| definition.name == "agent_create")
        .expect("agent_create definition should exist");
    assert_eq!(
        create.parameters["required"],
        json!(["profile"]),
        "agent_create should require only the built-in profile"
    );
    assert_eq!(
        create.parameters["properties"]["profile"]["enum"],
        json!([
            "codex", "claude", "opencode", "infcode", "cursor", "grok", "kimi", "zcode", "pi",
            "hermes"
        ]),
        "agent_create profile enum 应包含且仅包含十种受支持 runtime，InfCode 不得遗漏"
    );
    assert_ne!(
        create.parameters["properties"]["profile"]["enum"],
        json!(["codex", "claude", "opencode", "cursor", "grok"]),
        "新增 InfCode 等 Agent 后 agent_create schema 不得继续停留在旧五种 profile"
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

/// 文字会话、豆包、语音和 GPT-Live 后台共用这一份可见性。
///
/// 数据构造：四种范围。个人无桌面、个人在桌面、普通项目、项目且桌面已开。
/// 执行过程：对同一批工具名调用 `model_tool_visible`。
/// 预期结果：记忆始终可见；个人无桌面只有记忆和共享上下文；个人在桌面增加快照、派活和想法；项目工具不进个人会话。
#[test]
fn model_tool_visibility_matches_doubao_voice_and_gpt_live_scopes() {
    let personal = ModelToolScope {
        personal: true,
        desktop: false,
        shared: true,
        mobile: false,
    };
    assert!(model_tool_visible("memory_read", personal), "记忆始终可见");
    assert!(
        model_tool_visible("context_read", personal),
        "豆包个人会话仍保留共享上下文工具"
    );
    assert!(
        !model_tool_visible("agent_assign", personal),
        "没有桌面的个人会话不能派活"
    );
    assert!(
        !model_tool_visible("terminal_create", personal),
        "个人会话没有项目卡片工具"
    );

    let personal_desktop = ModelToolScope {
        desktop: true,
        ..personal
    };
    for name in [
        "workspace_snapshot",
        "agent_assign",
        "agent_show",
        "file_open",
        "idea_save",
        "idea_list",
    ] {
        assert!(
            model_tool_visible(name, personal_desktop),
            "个人桌面会话应看到 {name}"
        );
    }
    for name in [
        "agent_create",
        "agent_reply",
        "terminal_create",
        "card_delete",
        "create_browser",
    ] {
        assert!(
            !model_tool_visible(name, personal_desktop),
            "个人桌面会话不应看到 {name}"
        );
    }

    let project = ModelToolScope {
        personal: false,
        desktop: false,
        shared: false,
        mobile: false,
    };
    assert!(
        model_tool_visible("agent_assign", project),
        "当前项目用呼叫名派活"
    );
    assert!(
        model_tool_visible("agent_reply", project),
        "当前项目可直接对话"
    );
    assert!(
        !model_tool_visible("workspace_snapshot", project),
        "桌面未开时不注册快照"
    );

    let project_desktop = ModelToolScope {
        desktop: true,
        ..project
    };
    assert!(
        model_tool_visible("agent_assign", project_desktop),
        "项目加桌面只注册一次派活"
    );
    assert!(
        model_tool_visible("idea_list", project_desktop),
        "桌面想法工具随桌面出现"
    );
}
