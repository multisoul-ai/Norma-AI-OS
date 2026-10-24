use super::*;
use canvas::AgentProfile;
use storage::{NewAgentDefinition, ProjectStore};

/// AS-20：取消 New Agent 应回到进入新建前的选择，而不是留下空 Inspector。
///
/// 数据构造（含关键数值的推导过程）：
///   definitions       = 2 条：Alan + Bella
///   selected before   = Bella.id
///   temporary draft   = 1 份，无持久化 id
///   definitions after = 2 条，取消不增不减
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Alan 与 Bella，绑定 store 后显式选择 Bella。
///   2. 调用 create_new_card 进入未保存的新建态。
///   3. 调用 cancel_card_edit 取消临时草稿。
///   4. 读取 selection、draft、creating 状态与 definitions。
///
/// 预期结果：
///   - 正断言：取消成功，重新选中 Bella 并恢复 Bella draft。
///   - 正断言：definitions 仍恰好为原来的 2 条。
///   - 负断言：不再处于 creating，draft 不得残留默认新名称 Cora。
#[test]
fn test_agent_definitions_state_cancel_new_restores_previous_selection() {
    let temp = tempfile::tempdir().expect("取消 New Agent 测试应创建临时目录");
    let project = ProjectStore::for_database_paths(
        &storage::DatabasePaths::isolated_test(temp.path()),
        temp.path(),
    )
    .expect("测试 project store 应打开");
    project
        .migrate()
        .expect("取消 New Agent 测试应完成 migration");
    let store = project.agent_definition_store();
    store
        .create(&NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "工程实现".to_string(),
        })
        .expect("测试应创建 Alan definition");
    let bella = store
        .create(&NewAgentDefinition {
            name: "Bella".to_string(),
            runtime_profile: AgentProfile::Claude,
            workflow_description: "需求分析".to_string(),
        })
        .expect("测试应创建 Bella definition");
    let mut state = AgentDefinitionsState::default();
    state.bind_project_store(store);
    assert!(
        state.select_definition(bella.id),
        "进入新建前应成功选择 Bella"
    );
    assert!(
        state.create_new_card(),
        "干净的 Bella 详情应允许进入 New Agent"
    );

    let temporary_name = state.draft().name.clone();
    let did_cancel = state.cancel_card_edit();

    assert!(did_cancel, "取消 New Agent 应返回成功");
    assert_eq!(
        state.selected_definition_id(),
        Some(bella.id),
        "取消 New Agent 后应恢复进入新建前选中的 Bella"
    );
    assert_eq!(
        state.draft().name,
        "Bella",
        "取消 New Agent 后 Inspector 应重新加载 Bella draft"
    );
    assert_eq!(
        state.definitions().len(),
        2,
        "取消未保存草稿不得新增或删除 definition，仍应恰好为 2 条"
    );
    assert!(
        !state.is_creating_new(),
        "取消后不得继续停留在 creating_new 状态"
    );
    assert_ne!(
        state.draft().name,
        temporary_name,
        "取消后 draft 不得继续残留临时默认名称"
    );
}
