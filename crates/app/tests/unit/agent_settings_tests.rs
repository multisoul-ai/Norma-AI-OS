use super::*;
use canvas::AgentProfile;
use storage::{AgentDefinitionUpdate, NewAgentDefinition, ProjectStore};

/// AS-1: AgentDefinitionsState 应拒绝空名称、空 workflow，以及超长文本，避免无效草稿进入后续 UI/存储流程。
///
/// 数据构造（含关键数值的推导过程）：
///   empty name                = ""，trim 后仍为空
///   empty workflow            = ""，trim 后仍为空
///   long name chars           = 81，超过上限 80 1 个字符
///   long workflow chars       = 2001，超过上限 2000 1 个字符
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建默认 AgentDefinitionsState，初始 draft 为空
///   2. 依次设置 draft.name / draft.workflow_description，覆盖空值与超长值场景
///   3. 每次调用 validate_draft()，读取错误消息
///
/// 预期结果：
///   - 正断言：空名称返回 "Agent name is required"
///   - 正断言：空 workflow 返回 "Workflow description is required"
///   - 正断言：81 字符名称返回 80 字上限错误
///   - 正断言：2001 字 workflow 返回 2000 字上限错误
///   - 负断言：任一错误消息都不应误报成其他字段的错误
#[test]
fn test_agent_definitions_state_rejects_empty_name_and_workflow() {
    let mut state = AgentDefinitionsState::default();

    state.set_draft_name("");
    state.set_draft_workflow_description("valid workflow");
    let empty_name = state.validate_draft();

    state.set_draft_name("Valid Agent");
    state.set_draft_workflow_description("");
    let empty_workflow = state.validate_draft();

    state.set_draft_name("n".repeat(81));
    state.set_draft_workflow_description("valid workflow");
    let long_name = state.validate_draft();

    state.set_draft_name("Valid Agent");
    state.set_draft_workflow_description("w".repeat(2001));
    let long_workflow = state.validate_draft();

    assert_eq!(
        empty_name,
        Err("Agent name is required".to_string()),
        "空名称应返回必填错误，避免 UI 保存匿名 agent"
    );
    assert_ne!(
        empty_name,
        Err("Workflow description is required".to_string()),
        "空名称错误不应误报为 workflow 错误"
    );
    assert_eq!(
        empty_workflow,
        Err("Workflow description is required".to_string()),
        "空 workflow 应返回必填错误"
    );
    assert_ne!(
        empty_workflow,
        Err("Agent name is required".to_string()),
        "空 workflow 错误不应误报为名称错误"
    );
    assert_eq!(
        long_name,
        Err("Agent name must be 80 characters or fewer".to_string()),
        "81 字符名称应命中 80 字上限校验"
    );
    assert_eq!(
        long_workflow,
        Err("Workflow description must be 2000 characters or fewer".to_string()),
        "2001 字 workflow 应命中 2000 字上限校验"
    );
}

/// AS-2: AgentDefinitionsState 绑定 project store 后应只加载当前 workspace 的本地 agent 定义，不能串库。
///
/// 数据构造（含关键数值的推导过程）：
///   workspace A defs = 1 条，name = "Workspace A Agent"
///   workspace B defs = 1 条，name = "Workspace B Agent"
///   两个 workspace 各有独立 .multisoul/project.sqlite
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建两个真实 temp workspace，并各自 migrate project store
///   2. 分别写入 1 条 agent definition 到 A/B 的本地数据库
///   3. 用同一 AgentDefinitionsState 先 bind A 再 bind B，分别读取 definitions()
///
/// 预期结果：
///   - 正断言：绑定 A 时只看到 A 的定义
///   - 正断言：绑定 B 时只看到 B 的定义
///   - 负断言：A 结果里不能出现 B 名称，B 结果里不能出现 A 名称
#[test]
fn test_agent_definitions_state_loads_project_local_definitions() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let workspace_a = temp.path().join("workspace-a");
    let workspace_b = temp.path().join("workspace-b");
    std::fs::create_dir(&workspace_a).expect("test should create workspace A");
    std::fs::create_dir(&workspace_b).expect("test should create workspace B");

    let project_a =
        ProjectStore::for_workspace(&workspace_a).expect("workspace A store should open");
    let project_b =
        ProjectStore::for_workspace(&workspace_b).expect("workspace B store should open");
    project_a
        .migrate()
        .expect("workspace A project store should migrate");
    project_b
        .migrate()
        .expect("workspace B project store should migrate");

    project_a
        .agent_definition_store()
        .create(&NewAgentDefinition {
            name: "Workspace A Agent".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "A workflow".to_string(),
        })
        .expect("workspace A definition should persist");
    project_b
        .agent_definition_store()
        .create(&NewAgentDefinition {
            name: "Workspace B Agent".to_string(),
            runtime_profile: AgentProfile::Claude,
            workflow_description: "B workflow".to_string(),
        })
        .expect("workspace B definition should persist");

    let mut state = AgentDefinitionsState::default();
    state.bind_project_store(project_a.agent_definition_store());
    let workspace_a_definitions = state.definitions().to_vec();

    state.bind_project_store(project_b.agent_definition_store());
    let workspace_b_definitions = state.definitions().to_vec();

    assert_eq!(
        workspace_a_definitions.len(),
        1,
        "绑定 workspace A 后应只加载 1 条本地 agent 定义"
    );
    assert_eq!(
        workspace_a_definitions[0].name, "Workspace A Agent",
        "workspace A 结果应返回 A 自己的 agent 定义"
    );
    assert_ne!(
        workspace_a_definitions[0].name, "Workspace B Agent",
        "workspace A 结果不应串到 workspace B 的 agent 名称"
    );
    assert_eq!(
        workspace_b_definitions.len(),
        1,
        "绑定 workspace B 后应只加载 1 条本地 agent 定义"
    );
    assert_eq!(
        workspace_b_definitions[0].name, "Workspace B Agent",
        "workspace B 结果应返回 B 自己的 agent 定义"
    );
    assert_ne!(
        workspace_b_definitions[0].name, "Workspace A Agent",
        "workspace B 结果不应残留 workspace A 的 agent 名称"
    );
}

/// AS-3: voice_context() 只应暴露 enabled 的 agent，避免禁用 agent 进入语音路由候选集。
///
/// 数据构造（含关键数值的推导过程）：
///   enabled agent  = 1 条，enabled = true
///   disabled agent = 1 条，create 后再 update 成 enabled = false
///   voice context expected count = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建真实 temp workspace，并 migrate project store
///   2. 写入 enabled agent 与 disabled agent
///   3. bind_project_store() 载入 definitions，再调用 voice_context()
///
/// 预期结果：
///   - 正断言：voice_context 只返回 enabled agent
///   - 正断言：返回项包含正确 id / name / runtime_profile / workflow
///   - 负断言：disabled agent 名称不应出现在 voice_context 中
#[test]
fn test_agent_voice_context_includes_enabled_agents_only() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let workspace = temp.path().join("voice-context-workspace");
    std::fs::create_dir(&workspace).expect("test should create workspace dir");
    let project = ProjectStore::for_workspace(&workspace).expect("project store should open");
    project
        .migrate()
        .expect("project store should migrate before writes");
    let store = project.agent_definition_store();

    let enabled = store
        .create(&NewAgentDefinition {
            name: "Enabled Agent".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "Enabled workflow".to_string(),
        })
        .expect("enabled definition should persist");
    let disabled = store
        .create(&NewAgentDefinition {
            name: "Disabled Agent".to_string(),
            runtime_profile: AgentProfile::Claude,
            workflow_description: "Disabled workflow".to_string(),
        })
        .expect("disabled definition should persist before update");
    store
        .update(&AgentDefinitionUpdate {
            id: disabled.id,
            name: disabled.name.clone(),
            runtime_profile: disabled.runtime_profile,
            workflow_description: disabled.workflow_description.clone(),
            enabled: false,
            sort_order: disabled.sort_order,
        })
        .expect("disabled definition should update to enabled=false");

    let mut state = AgentDefinitionsState::default();
    state.bind_project_store(store);
    let voice_context = state.voice_context();

    assert_eq!(
        voice_context.len(),
        1,
        "voice_context 应只返回启用的 agent，禁用项必须被过滤"
    );
    assert_eq!(
        voice_context[0].id, enabled.id,
        "voice_context 返回的 agent id 应对应启用定义"
    );
    assert_eq!(
        voice_context[0].name, "Enabled Agent",
        "voice_context 应保留启用 agent 的名称"
    );
    assert_eq!(
        voice_context[0].runtime_profile,
        AgentProfile::Codex,
        "voice_context 应保留启用 agent 的 runtime profile"
    );
    assert_eq!(
        voice_context[0].workflow_description, "Enabled workflow",
        "voice_context 应保留启用 agent 的 workflow 描述"
    );
    assert!(
        !voice_context
            .iter()
            .any(|entry| entry.name == "Disabled Agent"),
        "禁用 agent 不应进入 voice_context，否则语音规划会看到已禁用定义"
    );
}
