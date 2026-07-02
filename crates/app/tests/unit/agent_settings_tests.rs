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

/// AS-4: settings::agents facade 应直接暴露 Task 2 承诺的草稿、字段和语音上下文类型，避免 Task 3 模块无法命名它们。
///
/// 数据构造（含关键数值的推导过程）：
///   AgentDefinitionDraft = 1 个实例，用于证明类型可从 facade 命名并构造
///   AgentDefinitionField = 1 个枚举值，用于证明字段枚举可从 facade 命名
///   AgentVoiceContext    = 1 个实例，用于证明语音上下文类型可从 facade 命名并构造
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从 super::agents facade 直接导入三个 promised 类型
///   2. 构造一个 draft、一个字段枚举值、一个 voice context
///   3. 断言这些值可被正常比较与读取，证明类型 re-export 生效
///
/// 预期结果：
///   - 正断言：draft、field、voice context 都能从 facade 命名并实例化
///   - 正断言：field 取值与 voice context 字段保持预期内容
///   - 负断言：类型不应再只能通过 private state 模块路径访问
#[test]
fn test_agent_settings_facade_reexports_task2_types() {
    use super::{AgentDefinitionDraft, AgentDefinitionField, AgentVoiceContext};
    use canvas::AgentProfile;

    let draft = AgentDefinitionDraft::new();
    let field = AgentDefinitionField::Name;
    let voice_context = AgentVoiceContext {
        id: 42,
        name: "Facade Agent".to_string(),
        runtime_profile: AgentProfile::Codex,
        workflow_description: "Facade workflow".to_string(),
    };

    assert_eq!(
        draft.name,
        String::new(),
        "draft should be constructible from the settings::agents facade"
    );
    assert_eq!(
        field,
        AgentDefinitionField::Name,
        "field enum should be accessible from the settings::agents facade"
    );
    assert_eq!(
        voice_context.id, 42,
        "voice context should be constructible from the settings::agents facade"
    );
    assert_eq!(
        voice_context.name, "Facade Agent",
        "voice context fields should remain readable after facade re-export"
    );
    assert_eq!(
        voice_context.workflow_description, "Facade workflow",
        "voice context workflow text should remain readable after facade re-export"
    );
}

/// AS-5: Agents 设置页应从空白占位升级为真实内容，至少暴露列表、表单和保存入口锚点。
///
/// 数据构造（含关键数值的推导过程）：
///   source files = content.rs + agents.rs + agents/view.rs（view.rs 在 Task 3 前不存在）
///   required ids  = 6 个：
///     settings-agents-list
///     settings-agent-name-input
///     settings-agent-workflow-input
///     settings-agent-profile-codex
///     settings-agent-save
///     settings-agent-empty-project
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 Settings content 与 agents facade 的源码
///   2. 尝试读取 agents/view.rs 真实视图源码
///   3. 断言关键 ids 已出现在实现中，证明 Agents 页面不再是空白占位
///
/// 预期结果：
///   - 正断言：所有 Agents 视图锚点都存在
///   - 负断言：缺少 view.rs 时测试必须失败，证明 Task 3 尚未实现
#[test]
fn test_agents_settings_section_renders_real_content() {
    let source = format!(
        "{}{}{}",
        include_str!("../../src/settings/content.rs"),
        include_str!("../../src/settings/agents.rs"),
        include_str!("../../src/settings/agents/view.rs")
    );
    let required = [
        "settings-agents-list",
        "settings-agent-name-input",
        "settings-agent-workflow-input",
        "settings-agent-profile-codex",
        "settings-agent-save",
        "settings-agent-empty-project",
    ];

    for anchor in required {
        assert!(
            source.contains(anchor),
            "Agents 设置页应包含真实内容锚点: {anchor}"
        );
    }
}

/// AS-6: 没有 project store 时，Agents 设置页应显示空项目提示并禁用“创建可保存 agent”路径。
///
/// 数据构造（含关键数值的推导过程）：
///   source files = agents/state.rs + agents/view.rs
///   no project condition = state.has_project_store() == false
///   required empty-state id = settings-agent-empty-project
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 AgentDefinitionsState 与 Agents view 源码
///   2. 检查 view 是否按 has_project_store() 分支渲染 empty-state
///   3. 检查 empty-state 文案存在，避免无项目时仍显示可操作表单
///
/// 预期结果：
///   - 正断言：view 会检查 has_project_store()
///   - 正断言：empty-state id 存在
///   - 负断言：无项目状态不应继续只有空白 div 占位
#[test]
fn test_agents_settings_no_project_state_disables_create_controls() {
    let source = format!(
        "{}{}",
        include_str!("../../src/settings/agents/state.rs"),
        include_str!("../../src/settings/agents/view.rs")
    );

    assert!(
        source.contains("has_project_store()"),
        "Agents view 应检查 has_project_store()，无项目时走 empty-state"
    );
    assert!(
        source.contains("settings-agent-empty-project"),
        "无项目时应渲染 settings-agent-empty-project 提示"
    );
    assert!(
        !source.contains("SettingsSection::Agents => div()"),
        "Agents 页面不应继续退回空白 div 占位"
    );
}

/// AS-7: Settings content 路由应把 Agents 分组接到 agents view，而不是继续回空白 div。
///
/// 数据构造（含关键数值的推导过程）：
///   source = settings/content.rs
///   target route = SettingsSection::Agents => agents_settings(...)
///   forbidden fallback = SettingsSection::Agents => div()
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 settings/content.rs 源码
///   2. 断言 Agents 分组已调用 agents_settings
///   3. 断言旧的空白 fallback 已被移除
///
/// 预期结果：
///   - 正断言：content 路由到 agents_settings
///   - 负断言：SettingsSection::Agents => div() 不存在
#[test]
fn test_agents_settings_content_routes_agents_section_to_agents_view() {
    let source = include_str!("../../src/settings/content.rs");

    assert!(
        source.contains("SettingsSection::Agents => agents_settings("),
        "Settings content 应把 Agents 分组路由到 agents_settings"
    );
    assert!(
        !source.contains(
            "SettingsSection::Agents\n                    | SettingsSection::Skills => div()"
        ) && !source.contains(
            "| SettingsSection::Agents\n                    | SettingsSection::Skills => div()"
        ) && !source.contains("SettingsSection::Agents => div()"),
        "SettingsSection::Agents 不应继续回退到空白 div"
    );
}
