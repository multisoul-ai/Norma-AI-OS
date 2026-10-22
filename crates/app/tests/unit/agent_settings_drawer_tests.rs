use super::*;
use canvas::AgentProfile;
use storage::{NewAgentDefinition, ProjectStore};

fn seeded_state(workspace_name: &str) -> (AgentDefinitionsState, i64, i64) {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let workspace = temp.path().join(workspace_name);
    std::fs::create_dir(&workspace).expect("test should create workspace dir");
    std::mem::forget(temp);
    let project = ProjectStore::for_workspace(&workspace).expect("project store should open");
    project.migrate().expect("project store should migrate");
    let store = project.agent_definition_store();
    let first = store
        .create(&NewAgentDefinition {
            name: "First".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "First workflow".to_string(),
        })
        .expect("test should seed first definition");
    let second = store
        .create(&NewAgentDefinition {
            name: "Second".to_string(),
            runtime_profile: AgentProfile::Claude,
            workflow_description: "Second workflow".to_string(),
        })
        .expect("test should seed second definition");
    let mut state = AgentDefinitionsState::default();
    state.bind_project_store(store);
    (state, first.id, second.id)
}

/// 抽屉默认收起；点击卡片（select_definition）打开，关闭后回到收起。
///
/// 数据构造：2 条 definition（First、Second），绑定后自动加载第一条草稿。
///
/// 执行过程：
///   1. 绑定后读取 is_drawer_open()。
///   2. select_definition(second) 后读取 is_drawer_open()。
///   3. close_drawer() 后再次读取。
///
/// 预期结果：
///   - 负断言：绑定后虽然已加载第一条草稿，抽屉仍是收起的（网格 3 列）。
///   - 正断言：选中卡片后抽屉打开，草稿为 Second。
///   - 正断言：无修改时关闭成功，抽屉收起。
#[test]
fn test_drawer_opens_on_select_and_closes_when_clean() {
    let (mut state, _, second) = seeded_state("drawer-open-close");

    assert!(
        !state.is_drawer_open(),
        "绑定后抽屉应默认收起，内容优先显示卡片网格"
    );
    assert!(
        state.select_definition(second),
        "无未保存修改时应能选中卡片"
    );
    assert!(state.is_drawer_open(), "选中卡片后抽屉应打开");
    assert_eq!(state.draft().name, "Second", "抽屉应展示被选中的 Agent");
    assert!(state.close_drawer(), "无修改时应允许关闭抽屉");
    assert!(!state.is_drawer_open(), "关闭后抽屉应收起");
}

/// 有未保存修改时关闭抽屉被拒绝，并保留草稿与提示；放弃后才能关闭。
///
/// 数据构造：选中 First 后把运行时从 Codex 改为 Grok（dirty）。
///
/// 预期结果：
///   - 负断言：dirty 时 close_drawer() 返回 false，抽屉仍打开，草稿仍为 Grok。
///   - 正断言：给出“先保存或放弃”的状态提示。
///   - 正断言：revert 后可以关闭。
#[test]
fn test_drawer_refuses_close_while_dirty() {
    let (mut state, first, _) = seeded_state("drawer-dirty-close");
    assert!(state.select_definition(first), "应能选中 First");
    state.set_draft_runtime_profile(AgentProfile::Grok);

    assert!(!state.close_drawer(), "有未保存修改时不得静默关闭抽屉");
    assert!(state.is_drawer_open(), "拒绝关闭后抽屉应保持打开");
    assert_eq!(
        state.draft().runtime_profile,
        AgentProfile::Grok,
        "拒绝关闭不得丢弃用户的草稿"
    );
    assert!(state.status().is_some(), "拒绝关闭应提示先保存或放弃");
    assert!(state.revert_draft(), "放弃修改应成功");
    assert!(state.close_drawer(), "放弃后应允许关闭");
    assert!(!state.is_drawer_open(), "放弃并关闭后抽屉应收起");
}

/// 新建草稿打开抽屉；未改动时关闭直接取消，用户改过则拒绝关闭以免静默丢稿。
///
/// 预期结果：
///   - 正断言：create_new_card() 后抽屉打开且处于新建态。
///   - 正断言：close_drawer() 成功，退出新建态并收起抽屉。
///   - 负断言：关闭后不再 is_creating_new()。
///   - 负断言：再次新建并修改运行时后，close_drawer() 返回 false，草稿保留。
///   - 正断言：用 cancel_card_edit()（页脚“取消”）仍可显式放弃。
#[test]
fn test_drawer_close_cancels_new_draft() {
    let (mut state, _, _) = seeded_state("drawer-new-cancel");

    assert!(state.create_new_card(), "应能进入新建态");
    assert!(state.is_drawer_open(), "新建时抽屉应打开");
    assert!(state.close_drawer(), "关闭新建抽屉应直接取消草稿");
    assert!(!state.is_creating_new(), "取消后不应残留新建态");
    assert!(!state.is_drawer_open(), "取消后抽屉应收起");

    assert!(state.create_new_card(), "应能再次进入新建态");
    state.set_draft_runtime_profile(AgentProfile::Grok);
    assert!(!state.close_drawer(), "新建草稿被修改后不得静默关闭");
    assert!(state.is_creating_new(), "拒绝关闭后应保留新建草稿");
    assert!(state.cancel_card_edit(), "页脚取消应能显式放弃草稿");
    assert!(!state.is_drawer_open(), "显式取消后抽屉应收起");
}

/// 删除当前 Agent 后抽屉收起，而不是自动跳到相邻 Agent。
///
/// 预期结果：
///   - 正断言：delete_definition 成功，definitions 剩 1 条。
///   - 负断言：抽屉不再打开。
#[test]
fn test_delete_closes_drawer() {
    let (mut state, first, _) = seeded_state("drawer-delete");
    assert!(state.select_definition(first), "应能选中 First");

    assert!(state.delete_definition(first), "删除应成功");
    assert_eq!(state.definitions().len(), 1, "删除后应只剩 1 条 definition");
    assert!(!state.is_drawer_open(), "删除后抽屉应收起");
}

/// 卡片开关立即持久化；有未保存草稿时拒绝，避免 reload 吞掉草稿。
///
/// 数据构造：First、Second 均启用；选中 First 并修改运行时使其 dirty。
///
/// 预期结果：
///   - 负断言：dirty 时对 Second 的开关返回 false，Second 仍启用。
///   - 正断言：放弃后开关成功，Second 被持久化为停用。
#[test]
fn test_card_switch_persists_and_respects_dirty_draft() {
    let (mut state, first, second) = seeded_state("drawer-card-switch");
    assert!(state.select_definition(first), "应能选中 First");
    state.set_draft_runtime_profile(AgentProfile::Grok);

    assert!(
        !state.toggle_enabled_from_card(second),
        "dirty 时不得切换其他卡片"
    );
    assert!(
        state
            .definitions()
            .iter()
            .any(|definition| definition.id == second && definition.enabled),
        "被拒绝后 Second 仍应启用"
    );
    assert!(state.revert_draft(), "放弃修改应成功");
    assert!(state.toggle_enabled_from_card(second), "干净状态下应能切换");
    assert!(
        state
            .definitions()
            .iter()
            .any(|definition| definition.id == second && !definition.enabled),
        "切换后 Second 应被持久化为停用"
    );
}
