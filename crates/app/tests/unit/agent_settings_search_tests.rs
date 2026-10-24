use super::*;
use canvas::AgentProfile;
use storage::{NewAgentDefinition, ProjectStore};

/// AS-2A：Agent 列表搜索应按名称或 Runtime Profile 过滤，并可清空恢复全部定义。
///
/// 数据构造（含关键数值的推导过程）：
///   definitions = 3 条：Alan/Codex、Bella/OpenCode、Quinn/Cursor
///   name query  = "quinn"，预期 1 条 Quinn
///   profile query = "cursor"，预期 1 条 Quinn
///   empty query = ""，预期恢复 3 条
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建真实临时 project store，并写入 3 条 Agent definition。
///   2. 绑定 AgentDefinitionsState，设置小写名称查询 quinn 并读取 visible_definitions。
///   3. 改为 Runtime Profile 查询 cursor，再读取过滤结果。
///   4. 清空查询，确认完整定义列表恢复且持久化数据未被删除。
///
/// 预期结果：
///   - 正断言：名称与 Runtime Profile 搜索都只返回 Quinn。
///   - 负断言：匹配结果不得包含 Alan 或 Bella。
///   - 正断言：清空后按原顺序恢复 Alan、Bella、Quinn。
///   - 负断言：重复设置相同空查询不得报告状态变化。
#[test]
fn test_agent_definitions_state_filters_visible_agents_without_mutating_definitions() {
    let temp = tempfile::tempdir().expect("Agent 搜索测试应创建临时目录");
    let project = ProjectStore::for_database_paths(
        &storage::DatabasePaths::isolated_test(temp.path()),
        temp.path(),
    )
    .expect("Agent 搜索 project store 应打开");
    project
        .migrate()
        .expect("Agent 搜索 project store 应完成 migration");
    let store = project.agent_definition_store();
    store
        .create(&NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "工程实现".to_string(),
        })
        .expect("测试应创建 Alan/Codex definition");
    store
        .create(&NewAgentDefinition {
            name: "Bella".to_string(),
            runtime_profile: AgentProfile::OpenCode,
            workflow_description: "代码审查".to_string(),
        })
        .expect("测试应创建 Bella/OpenCode definition");
    store
        .create(&NewAgentDefinition {
            name: "Quinn".to_string(),
            runtime_profile: AgentProfile::Cursor,
            workflow_description: "交互验证".to_string(),
        })
        .expect("测试应创建 Quinn/Cursor definition");

    let mut state = AgentDefinitionsState::default();
    state.bind_project_store(store);
    assert!(
        state.set_search_query("quinn".to_string()),
        "从空查询切换为 quinn 应报告搜索状态已变化"
    );
    let name_matches = state
        .visible_definitions()
        .into_iter()
        .map(|definition| definition.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        name_matches.as_slice(),
        ["Quinn"],
        "小写名称查询 quinn 应大小写不敏感地只返回 Quinn"
    );
    assert!(
        !name_matches
            .iter()
            .any(|name| *name == "Alan" || *name == "Bella"),
        "名称查询 quinn 不得包含 Alan 或 Bella"
    );

    assert!(
        state.set_search_query("cursor".to_string()),
        "从 quinn 切换为 cursor 应报告搜索状态已变化"
    );
    let profile_matches = state
        .visible_definitions()
        .into_iter()
        .map(|definition| definition.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        profile_matches.as_slice(),
        ["Quinn"],
        "Runtime Profile 查询 cursor 应只返回 Quinn/Cursor"
    );
    assert!(
        !profile_matches
            .iter()
            .any(|name| *name == "Alan" || *name == "Bella"),
        "Runtime Profile 查询 cursor 不得包含 Alan 或 Bella"
    );

    assert!(
        state.set_search_query(String::new()),
        "从 cursor 清空查询应报告搜索状态已变化"
    );
    let restored = state
        .visible_definitions()
        .into_iter()
        .map(|definition| definition.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        restored.as_slice(),
        ["Alan", "Bella", "Quinn"],
        "清空搜索后应按原顺序恢复全部 3 条定义"
    );
    assert_eq!(
        state.definitions().len(),
        3,
        "搜索和清空过程不得删除任何持久化 Agent definition"
    );
    assert!(
        !state.set_search_query(String::new()),
        "重复设置相同空查询不得误报状态变化"
    );
}

/// AS-2B：Agent 状态筛选应与搜索组合，并精确区分 Enabled 与 Disabled。
///
/// 数据构造（含关键数值的推导过程）：
///   definitions      = 3 条：Alan(enabled)、Bella(disabled)、Quinn(enabled)
///   disabled filter  = 3 条中的 1 条，预期只返回 Bella
///   workflow query   = "审查"，与 disabled filter 交集仍为 1 条 Bella
///   enabled filter   = 3 - 1(disabled) = 2 条，预期 Alan + Quinn
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建三条 definition，并把 Bella 的 enabled 更新为 false。
///   2. 设置 Disabled filter，读取 visible_definitions。
///   3. 追加 workflow 查询“审查”，确认搜索与筛选取交集。
///   4. 清空搜索并切到 Enabled filter，读取剩余结果。
///
/// 预期结果：
///   - 正断言：Disabled 只返回 Bella，且 workflow 查询可匹配 Bella。
///   - 负断言：Disabled 不得包含 Alan 或 Quinn。
///   - 正断言：Enabled 恢复 Alan 与 Quinn 两条。
///   - 负断言：Enabled 不得残留 Bella。
#[test]
fn test_agent_definitions_state_combines_status_filter_with_workflow_search() {
    let temp = tempfile::tempdir().expect("Agent 状态筛选测试应创建临时目录");
    let project = ProjectStore::for_database_paths(
        &storage::DatabasePaths::isolated_test(temp.path()),
        temp.path(),
    )
    .expect("Agent 状态筛选 project store 应打开");
    project
        .migrate()
        .expect("Agent 状态筛选 project store 应完成 migration");
    let store = project.agent_definition_store();
    store
        .create(&NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "工程实现".to_string(),
        })
        .expect("测试应创建 enabled 的 Alan definition");
    let bella = store
        .create(&NewAgentDefinition {
            name: "Bella".to_string(),
            runtime_profile: AgentProfile::OpenCode,
            workflow_description: "负责代码审查".to_string(),
        })
        .expect("测试应创建待禁用的 Bella definition");
    store
        .update(&storage::AgentDefinitionUpdate {
            id: bella.id,
            name: bella.name,
            runtime_profile: bella.runtime_profile,
            workflow_description: bella.workflow_description,
            enabled: false,
            sort_order: bella.sort_order,
        })
        .expect("测试应把 Bella 更新为 disabled");
    store
        .create(&NewAgentDefinition {
            name: "Quinn".to_string(),
            runtime_profile: AgentProfile::Cursor,
            workflow_description: "交互验证".to_string(),
        })
        .expect("测试应创建 enabled 的 Quinn definition");

    let mut state = AgentDefinitionsState::default();
    state.bind_project_store(store);
    assert!(
        state.set_status_filter(AgentStatusFilter::Disabled),
        "从 All 切换为 Disabled 应报告筛选状态变化"
    );
    let disabled = state
        .visible_definitions()
        .into_iter()
        .map(|definition| definition.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        disabled.as_slice(),
        ["Bella"],
        "Disabled filter 应只返回唯一被禁用的 Bella"
    );
    assert!(
        !disabled
            .iter()
            .any(|name| *name == "Alan" || *name == "Quinn"),
        "Disabled filter 不得包含 enabled 的 Alan 或 Quinn"
    );

    assert!(
        state.set_search_query("审查".to_string()),
        "在 Disabled filter 下追加 workflow 查询应报告搜索状态变化"
    );
    let workflow_match = state
        .visible_definitions()
        .into_iter()
        .map(|definition| definition.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        workflow_match.as_slice(),
        ["Bella"],
        "搜索应匹配 workflow，并与 Disabled filter 组合后只返回 Bella"
    );

    assert!(
        state.set_search_query(String::new()),
        "清空 workflow 查询应报告搜索状态变化"
    );
    assert!(
        state.set_status_filter(AgentStatusFilter::Enabled),
        "从 Disabled 切换到 Enabled 应报告筛选状态变化"
    );
    let enabled = state
        .visible_definitions()
        .into_iter()
        .map(|definition| definition.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        enabled.as_slice(),
        ["Alan", "Quinn"],
        "Enabled filter 应按原顺序返回 Alan 与 Quinn"
    );
    assert!(
        !enabled.contains(&"Bella"),
        "Enabled filter 不得残留 disabled 的 Bella"
    );
}
