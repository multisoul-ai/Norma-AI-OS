use super::*;
use canvas::AgentProfile;
use storage::{NewAgentDefinition, ProjectStore};

fn make_agent_definition_store(workspace_name: &str) -> storage::AgentDefinitionStore {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let workspace_root = temp.path().to_path_buf();
    std::mem::forget(temp);
    let workspace = workspace_root.join(workspace_name);
    std::fs::create_dir(&workspace).expect("test should create workspace dir");
    let project = ProjectStore::for_workspace(&workspace).expect("project store should open");
    project
        .migrate()
        .expect("project store should migrate before writes");
    project.agent_definition_store()
}

/// AS-13: 绑定 project store 后，Agents 详情应自动选中第一条 definition；空项目进入新建草稿模式。
///
/// 数据构造（含关键数值的推导过程）：
///   populated store definitions = 2 条，按 store.list() 顺序返回
///   empty store definitions     = 0 条
///   selected id expected        = first.id（第一条 definition）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 populated project store，并写入 "First Agent" 与 "Second Agent"
///   2. bind_project_store(populated) 后读取 selected_definition_id() 与 draft()
///   3. 创建 empty project store，bind 后再次读取 selected_definition_id() 与 draft()
///
/// 预期结果：
///   - 正断言：有 definitions 时自动选中第一条 definition
///   - 正断言：选中后 draft 内容来自第一条 definition
///   - 正断言：空项目 selected_definition_id() 为 None
///   - 负断言：空项目不应残留 populated store 的名称
#[test]
fn test_agent_definitions_state_bind_selects_first_definition_or_new_mode() {
    let populated_store = make_agent_definition_store("task-6-selected-populated");
    let first = populated_store
        .create(&NewAgentDefinition {
            name: "First Agent".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "First workflow".to_string(),
        })
        .expect("test should seed first definition");
    populated_store
        .create(&NewAgentDefinition {
            name: "Second Agent".to_string(),
            runtime_profile: AgentProfile::Claude,
            workflow_description: "Second workflow".to_string(),
        })
        .expect("test should seed second definition");
    let empty_store = make_agent_definition_store("task-6-selected-empty");
    let mut state = AgentDefinitionsState::default();

    state.bind_project_store(populated_store);
    let populated_selected = state.selected_definition_id();
    let populated_draft = state.draft().clone();

    state.bind_project_store(empty_store);
    let empty_selected = state.selected_definition_id();
    let empty_draft = state.draft().clone();

    assert_eq!(
        populated_selected,
        Some(first.id),
        "绑定已有 definitions 的项目后应自动选中第一条，供右侧详情直接显示"
    );
    assert_eq!(
        populated_draft.name, "First Agent",
        "自动选中后 draft 应加载第一条 definition 的名称"
    );
    assert_eq!(
        populated_draft.workflow_description, "First workflow",
        "自动选中后 draft 应加载第一条 definition 的 workflow"
    );
    assert_eq!(
        empty_selected, None,
        "绑定空项目后应进入新建模式，而不是保留旧项目 selected id"
    );
    assert_ne!(
        empty_draft.name, "First Agent",
        "绑定空项目后不应残留前一个项目的 draft 名称"
    );
}

/// AS-14: 详情草稿变脏后，切换选择或新建应被阻止，避免用户未保存的 workflow 被覆盖。
///
/// 数据构造（含关键数值的推导过程）：
///   definitions              = 2 条，first.id 与 second.id 都存在
///   dirty edit               = draft.name 从 "First Agent" 改成 "Unsaved Name"
///   expected selected id     = first.id，切换失败后仍保持原选中项
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建真实 project store 并写入两条 definition
///   2. bind 后自动选中 first，再修改 draft.name 形成 dirty 状态
///   3. 调用 select_definition(second.id) 与 begin_new_definition()
///   4. 读取 selected_definition_id()、draft()、status()
///
/// 预期结果：
///   - 正断言：dirty 状态为 true
///   - 正断言：切换到 second 返回 false
///   - 正断言：新建返回 false
///   - 正断言：selected id 仍是 first.id
///   - 负断言：draft 不应被 second 或空新建草稿覆盖
#[test]
fn test_agent_definitions_state_dirty_draft_blocks_selection_and_new_agent() {
    let store = make_agent_definition_store("task-6-dirty-guard");
    let first = store
        .create(&NewAgentDefinition {
            name: "First Agent".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "First workflow".to_string(),
        })
        .expect("test should seed first definition");
    let second = store
        .create(&NewAgentDefinition {
            name: "Second Agent".to_string(),
            runtime_profile: AgentProfile::Claude,
            workflow_description: "Second workflow".to_string(),
        })
        .expect("test should seed second definition");
    let mut state = AgentDefinitionsState::default();
    state.bind_project_store(store);

    state.set_draft_name("Unsaved Name");
    let dirty_before_switch = state.is_dirty();
    let selected_second = state.select_definition(second.id);
    let began_new = state.begin_new_definition();
    let selected_after = state.selected_definition_id();
    let draft_after = state.draft().clone();
    let status_after = state.status().map(str::to_string);

    assert!(
        dirty_before_switch,
        "修改当前详情 draft 后 is_dirty() 应为 true，供 UI 阻止破坏性切换"
    );
    assert!(
        !selected_second,
        "dirty draft 存在时 select_definition(second.id) 应返回 false"
    );
    assert!(
        !began_new,
        "dirty draft 存在时 begin_new_definition() 应返回 false"
    );
    assert_eq!(
        selected_after,
        Some(first.id),
        "dirty 切换失败后 selected id 应仍保持 first.id"
    );
    assert_eq!(
        draft_after.name, "Unsaved Name",
        "dirty 切换失败后 draft 应保留用户未保存名称"
    );
    assert_ne!(
        draft_after.name, "Second Agent",
        "dirty 切换失败后 draft 不应被 second definition 覆盖"
    );
    assert_eq!(
        status_after.as_deref(),
        Some("Save or revert the current Agent before switching."),
        "dirty 切换失败应给出明确保存或恢复提示"
    );
}

/// AS-15: 保存新建草稿后，应选中新建出的 definition，而不是清空右侧详情。
///
/// 数据构造（含关键数值的推导过程）：
///   initial definitions       = 0 条
///   draft name                = "New Detail Agent"，长度 16 <= 80
///   draft workflow            = "New detail workflow"，长度 19 <= 2000
///   final definitions         = 1 条，新建后应被选中
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建空 project store 并 bind
///   2. 设置合法新建草稿字段后调用 save_draft()
///   3. 读取 definitions()、selected_definition_id() 与 draft()
///
/// 预期结果：
///   - 正断言：save_draft() 返回 true
///   - 正断言：definitions 中有 1 条新记录
///   - 正断言：selected_definition_id() 指向新记录 id
///   - 负断言：draft 不应被重置成空名称
#[test]
fn test_agent_definitions_state_save_new_draft_selects_created_definition() {
    let store = make_agent_definition_store("task-6-save-select-created");
    let mut state = AgentDefinitionsState::default();
    state.bind_project_store(store);

    state.set_draft_name("New Detail Agent");
    state.set_draft_runtime_profile(AgentProfile::Claude);
    state.set_draft_workflow_description("New detail workflow");
    let did_save = state.save_draft();
    let definitions = state.definitions().to_vec();
    let selected = state.selected_definition_id();
    let draft = state.draft().clone();

    assert!(
        did_save,
        "有效新建草稿应保存成功，供详情页生成第一条 Agent 模板"
    );
    assert_eq!(
        definitions.len(),
        1,
        "保存新建草稿后 definitions 应新增 1 条记录"
    );
    assert_eq!(
        selected,
        Some(definitions[0].id),
        "保存新建草稿后应选中新建 definition，右侧详情继续显示该对象"
    );
    assert_eq!(
        draft.name, "New Detail Agent",
        "保存新建草稿后 draft 应显示新建 definition 名称"
    );
    assert_ne!(
        draft.name, "",
        "保存新建草稿后 draft 不应被重置为空，否则详情区域会突然变成空表单"
    );
}

/// AS-16: Revert 应恢复当前选中 definition 的持久化字段，并清除 dirty 状态。
///
/// 数据构造（含关键数值的推导过程）：
///   seeded definition name     = "Persisted Agent"
///   dirty draft name           = "Temporary Name"
///   expected reverted name     = "Persisted Agent"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 project store 并写入 1 条 definition
///   2. bind 后修改 draft.name，让详情变 dirty
///   3. 调用 revert_draft()，读取 draft() 与 is_dirty()
///
/// 预期结果：
///   - 正断言：revert_draft() 返回 true
///   - 正断言：draft 名称恢复为持久化名称
///   - 正断言：is_dirty() 恢复为 false
///   - 负断言：临时名称不应继续留在 draft 中
#[test]
fn test_agent_definitions_state_revert_restores_selected_definition() {
    let store = make_agent_definition_store("task-6-revert");
    store
        .create(&NewAgentDefinition {
            name: "Persisted Agent".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "Persisted workflow".to_string(),
        })
        .expect("test should seed persisted definition");
    let mut state = AgentDefinitionsState::default();
    state.bind_project_store(store);

    state.set_draft_name("Temporary Name");
    let dirty_before_revert = state.is_dirty();
    let did_revert = state.revert_draft();
    let draft_after = state.draft().clone();
    let dirty_after_revert = state.is_dirty();

    assert!(
        dirty_before_revert,
        "修改 draft.name 后 is_dirty() 应为 true，确保测试覆盖真实 revert 场景"
    );
    assert!(
        did_revert,
        "选中已有 definition 时 revert_draft() 应返回 true"
    );
    assert_eq!(
        draft_after.name, "Persisted Agent",
        "revert 后 draft 名称应恢复为持久化 definition 名称"
    );
    assert!(
        !dirty_after_revert,
        "revert 后 is_dirty() 应为 false，Save/Revert 按钮可回到静默状态"
    );
    assert_ne!(
        draft_after.name, "Temporary Name",
        "revert 后临时名称不应继续留在 draft 中"
    );
}

/// AS-17: 删除选中 Agent 必须两次确认；确认删除后选择下一条可用 definition。
///
/// 数据构造（含关键数值的推导过程）：
///   definitions before delete = 2 条，Delete Me 与 Keep Me
///   first delete click        = 只设置 confirmation，不删除
///   second delete click       = 删除 Delete Me
///   definitions after delete  = 1 条，Keep Me 被选中
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 project store 并写入 Delete Me 与 Keep Me
///   2. bind 后选中 Delete Me
///   3. 连续调用 request_delete_definition(Delete Me.id) 两次
///   4. 读取 definitions()、selected_definition_id() 与 draft()
///
/// 预期结果：
///   - 正断言：第一次 delete 返回 false 且不删除记录
///   - 正断言：第二次 delete 返回 true 且删除目标记录
///   - 正断言：删除后 selected id 指向 Keep Me
///   - 负断言：Delete Me 不应继续出现在内存列表或 draft 中
#[test]
fn test_agent_definitions_state_delete_requires_confirmation_and_selects_next() {
    let store = make_agent_definition_store("task-6-confirm-delete");
    let deleted = store
        .create(&NewAgentDefinition {
            name: "Delete Me".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "Delete workflow".to_string(),
        })
        .expect("test should seed deleted definition");
    let kept = store
        .create(&NewAgentDefinition {
            name: "Keep Me".to_string(),
            runtime_profile: AgentProfile::Claude,
            workflow_description: "Keep workflow".to_string(),
        })
        .expect("test should seed kept definition");
    let mut state = AgentDefinitionsState::default();
    state.bind_project_store(store);

    let first_delete = state.request_delete_definition(deleted.id);
    let after_first_count = state.definitions().len();
    let second_delete = state.request_delete_definition(deleted.id);
    let definitions_after = state.definitions().to_vec();
    let selected_after = state.selected_definition_id();
    let draft_after = state.draft().clone();

    assert!(
        !first_delete,
        "第一次 request_delete_definition 应只进入确认状态，不立即删除"
    );
    assert_eq!(
        after_first_count, 2,
        "第一次 delete 确认前 definitions 数量应保持 2 条"
    );
    assert!(
        second_delete,
        "第二次 request_delete_definition 应确认并删除目标 definition"
    );
    assert_eq!(
        definitions_after.len(),
        1,
        "确认删除后 definitions 应只剩 1 条记录"
    );
    assert_eq!(
        selected_after,
        Some(kept.id),
        "删除当前选中 definition 后应自动选中下一条可用 definition"
    );
    assert_eq!(
        draft_after.name, "Keep Me",
        "删除后 draft 应加载新选中的 Keep Me 详情"
    );
    assert!(
        !definitions_after
            .iter()
            .any(|definition| definition.name == "Delete Me"),
        "确认删除后 Delete Me 不应继续出现在 definitions 列表中"
    );
    assert_ne!(
        draft_after.name, "Delete Me",
        "确认删除后 draft 不应继续显示已删除 definition"
    );
}

/// AS-19: 点击 + New 应立即创建一张持久化 Agent 卡片，并让该卡片进入编辑模式。
///
/// 数据构造（含关键数值的推导过程）：
///   initial definitions       = 0 条
///   default name              = "New Agent"，长度 9 <= 80
///   default workflow          = "Describe this agent's workflow."，非空且长度 31 <= 2000
///   final definitions         = 1 条，创建后应被选中并进入 editing_card_id
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建空 project store 并 bind
///   2. 调用 create_new_card()
///   3. 读取 definitions()、selected_definition_id()、editing_card_id() 与 store.list()
///
/// 预期结果：
///   - 正断言：create_new_card() 返回 true
///   - 正断言：内存与持久化都新增 1 条默认 Agent definition
///   - 正断言：selected_definition_id() 与 editing_card_id() 都指向新 definition
///   - 负断言：新 definition 不应停留在空名称或空 workflow，避免违反保存校验
#[test]
fn test_agent_definitions_state_create_new_card_persists_and_enters_edit_mode() {
    let store = make_agent_definition_store("task-7-create-card-inline-edit");
    let mut state = AgentDefinitionsState::default();
    state.bind_project_store(store.clone());

    let did_create = state.create_new_card();
    let definitions = state.definitions().to_vec();
    let persisted = store
        .list()
        .expect("store list should load auto-created card definition");
    let selected = state.selected_definition_id();
    let editing = state.editing_card_id();

    assert!(
        did_create,
        "+ New 应立即创建默认 Agent definition，而不是只打开空草稿"
    );
    assert_eq!(
        definitions.len(),
        1,
        "+ New 后内存 definitions 应新增 1 张 Agent 卡片"
    );
    assert_eq!(
        persisted.len(),
        1,
        "+ New 后 project store 应持久化 1 条 Agent definition"
    );
    assert_eq!(
        selected,
        Some(definitions[0].id),
        "+ New 后应选中新创建的 Agent 卡片"
    );
    assert_eq!(
        editing,
        Some(definitions[0].id),
        "+ New 后新卡片应立即进入内联编辑模式"
    );
    assert_ne!(
        definitions[0].name, "",
        "自动创建的 Agent definition 不应使用空名称，否则保存校验会失败"
    );
    assert_ne!(
        definitions[0].workflow_description, "",
        "自动创建的 Agent definition 不应使用空 workflow，否则保存校验会失败"
    );
}
