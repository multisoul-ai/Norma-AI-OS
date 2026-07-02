use super::*;
use canvas::AgentProfile;
use gpui::{Keystroke, Modifiers};
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

fn key(key: &str, key_char: Option<&str>) -> Keystroke {
    Keystroke {
        modifiers: Modifiers::default(),
        key: key.to_string(),
        key_char: key_char.map(str::to_string),
    }
}

/// AS-8: save_draft() 应把一个有效草稿创建为真实 definition，并同时刷新内存列表与底层 project store。
///
/// 数据构造（含关键数值的推导过程）：
///   initial definitions         = 0 条，验证创建前为空
///   draft name                 = "Catalog Agent"，长度 13 <= 80
///   draft workflow             = "Handle project catalog actions"，长度 30 <= 2000
///   runtime_profile            = Claude
///   persisted definitions      = 1 条，验证 save_draft() 成功创建
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建真实 temp project store，并 bind 到 AgentDefinitionsState
///   2. 设置合法 draft.name / runtime_profile / workflow_description
///   3. 调用 save_draft()，随后分别读取 state.definitions() 与 store.list()
///
/// 预期结果：
///   - 正断言：save_draft() 返回 true
///   - 正断言：内存 definitions 列表新增 1 条并保留输入字段
///   - 正断言：store.list() 也持久化出同一条 definition
///   - 负断言：保存成功后 draft 不应继续保留旧名称，避免脏草稿残留
#[test]
fn test_agent_definitions_state_save_draft_creates_definition() {
    let store = make_agent_definition_store("task-5-save-draft");
    let mut state = AgentDefinitionsState::default();
    state.bind_project_store(store.clone());

    state.set_draft_name("Catalog Agent");
    state.set_draft_runtime_profile(AgentProfile::Claude);
    state.set_draft_workflow_description("Handle project catalog actions");

    let did_save = state.save_draft();
    let in_memory = state.definitions().to_vec();
    let persisted = store.list().expect("store list should load saved definitions");

    assert!(did_save, "save_draft should return true for a valid draft");
    assert_eq!(
        in_memory.len(),
        1,
        "state definitions should contain exactly one created definition after save"
    );
    assert_eq!(
        in_memory[0].name, "Catalog Agent",
        "state definitions should include the saved agent name"
    );
    assert_eq!(
        in_memory[0].runtime_profile,
        AgentProfile::Claude,
        "state definitions should include the saved runtime profile"
    );
    assert_eq!(
        in_memory[0].workflow_description, "Handle project catalog actions",
        "state definitions should include the saved workflow description"
    );
    assert_eq!(
        persisted.len(),
        1,
        "project store should persist exactly one definition after save"
    );
    assert_eq!(
        persisted[0].name, "Catalog Agent",
        "project store should persist the created agent name"
    );
    assert_eq!(
        persisted[0].runtime_profile,
        AgentProfile::Claude,
        "project store should persist the created runtime profile"
    );
    assert_eq!(
        persisted[0].workflow_description, "Handle project catalog actions",
        "project store should persist the created workflow description"
    );
    assert_ne!(
        state.draft().name, "Catalog Agent",
        "draft name should reset after a successful save to avoid stale form state"
    );
}

/// AS-9: edit_definition() 加载已有 definition 后，再次 save_draft() 应更新原记录，而不是新增重复记录。
///
/// 数据构造（含关键数值的推导过程）：
///   initial definitions        = 1 条，name = "Original Agent"
///   edited name                = "Edited Agent"，长度 12 <= 80
///   edited workflow            = "Updated workflow"，长度 16 <= 2000
///   final persisted count      = 1 条，验证 update 而非 create
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建真实 temp project store，并先写入 1 条初始 definition
///   2. bind_project_store() 后调用 edit_definition(id) 把旧值载入 draft
///   3. 修改 draft 字段并调用 save_draft()，最后读取 state.definitions() 与 store.list()
///
/// 预期结果：
///   - 正断言：edit_definition() 返回 true 并把旧 definition 装入 draft
///   - 正断言：save_draft() 返回 true，内存与持久化都只保留 1 条更新后的记录
///   - 正断言：更新后记录仍使用同一个 id
///   - 负断言：旧名称 "Original Agent" 不应继续留在内存或持久化列表里
#[test]
fn test_agent_definitions_state_edit_and_save_updates_definition() {
    let store = make_agent_definition_store("task-5-edit-save");
    let original = store
        .create(&NewAgentDefinition {
            name: "Original Agent".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "Original workflow".to_string(),
        })
        .expect("test should seed original definition");
    let mut state = AgentDefinitionsState::default();
    state.bind_project_store(store.clone());

    let did_edit = state.edit_definition(original.id);
    state.set_draft_name("Edited Agent");
    state.set_draft_runtime_profile(AgentProfile::Claude);
    state.set_draft_workflow_description("Updated workflow");
    let did_save = state.save_draft();
    let in_memory = state.definitions().to_vec();
    let persisted = store.list().expect("store list should load updated definitions");

    assert!(
        did_edit,
        "edit_definition should return true for an existing definition id"
    );
    assert!(did_save, "save_draft should return true when updating a valid draft");
    assert_eq!(
        in_memory.len(),
        1,
        "state definitions should still contain one record after updating an existing definition"
    );
    assert_eq!(
        in_memory[0].id, original.id,
        "updated in-memory definition should keep the original id"
    );
    assert_eq!(
        in_memory[0].name, "Edited Agent",
        "updated in-memory definition should use the edited name"
    );
    assert_eq!(
        in_memory[0].runtime_profile,
        AgentProfile::Claude,
        "updated in-memory definition should use the edited runtime profile"
    );
    assert_eq!(
        in_memory[0].workflow_description, "Updated workflow",
        "updated in-memory definition should use the edited workflow"
    );
    assert_eq!(
        persisted.len(),
        1,
        "project store should still contain one record after updating an existing definition"
    );
    assert_eq!(
        persisted[0].id, original.id,
        "persisted update should keep the original definition id"
    );
    assert_eq!(
        persisted[0].name, "Edited Agent",
        "project store should persist the edited name instead of creating a second row"
    );
    assert!(
        !persisted.iter().any(|definition| definition.name == "Original Agent"),
        "project store should not retain the original name after update"
    );
    assert!(
        !in_memory.iter().any(|definition| definition.name == "Original Agent"),
        "state definitions should not retain the original name after update"
    );
}

/// AS-9A: save_draft() 遇到正在编辑中的字段时，必须先提交可见 edit_buffer，再做校验和持久化，避免 Save 丢失用户眼前已输入但未按 Enter 的文本。
///
/// 数据构造（含关键数值的推导过程）：
///   seeded definitions        = 1 条，name = "Original Agent"
///   visible pending name      = "Visible Draft Name"，长度 18 <= 80
///   stored draft name before save = "Original Agent"，验证 save 前 self.draft 仍是旧值
///   final persisted count     = 1 条，验证 update 而非 create
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建真实 temp project store，并先写入 1 条初始 definition
///   2. bind_project_store() 后调用 edit_definition(id)，让 draft 载入旧值
///   3. begin_edit(Name) 后逐字输入 "Visible Draft Name"，故意不按 Enter
///   4. 直接调用 save_draft()，随后读取 state.definitions() 与 store.list()
///
/// 预期结果：
///   - 正断言：save_draft() 返回 true，说明保存路径接受了当前可见输入
///   - 正断言：内存与持久化中的名称都更新为 "Visible Draft Name"
///   - 正断言：仍只有 1 条记录，说明是更新不是新增
///   - 负断言：旧名称 "Original Agent" 不应继续留在内存或持久化列表中
#[test]
fn test_agent_definitions_state_save_draft_commits_pending_active_edit_buffer() {
    let store = make_agent_definition_store("task-5-pending-edit-save");
    let original = store
        .create(&NewAgentDefinition {
            name: "Original Agent".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "Original workflow".to_string(),
        })
        .expect("test should seed original definition");
    let mut state = AgentDefinitionsState::default();
    state.bind_project_store(store.clone());

    let did_edit = state.edit_definition(original.id);
    state.begin_edit(AgentDefinitionField::Name);
    for _ in 0.."Original Agent".chars().count() {
        state.handle_key_down(&key("backspace", None));
    }
    for ch in "Visible Draft Name".chars() {
        let keystroke = if ch == ' ' {
            key("space", Some(" "))
        } else {
            let text = ch.to_string();
            key(&text, Some(&text))
        };
        state.handle_key_down(&keystroke);
    }

    let did_save = state.save_draft();
    let in_memory = state.definitions().to_vec();
    let persisted = store
        .list()
        .expect("store list should load updated definitions after save");

    assert!(
        did_edit,
        "edit_definition should return true so the test exercises an existing definition"
    );
    assert!(
        did_save,
        "save_draft should return true after committing the visible pending edit buffer"
    );
    assert_eq!(
        in_memory.len(),
        1,
        "state definitions should still contain one record after saving a pending in-place edit"
    );
    assert_eq!(
        in_memory[0].name, "Visible Draft Name",
        "state definitions should persist the visible edit buffer text instead of the stale draft name"
    );
    assert!(
        !in_memory.iter().any(|definition| definition.name == "Original Agent"),
        "state definitions should not retain the stale pre-edit name after save"
    );
    assert_eq!(
        persisted.len(),
        1,
        "project store should still contain one record after saving a pending in-place edit"
    );
    assert_eq!(
        persisted[0].name, "Visible Draft Name",
        "project store should persist the visible edit buffer text instead of the stale draft name"
    );
    assert!(
        !persisted.iter().any(|definition| definition.name == "Original Agent"),
        "project store should not retain the stale pre-edit name after save"
    );
}

/// AS-10: delete_definition() 应同时删除内存与底层 store 的 definition，并返回 true 表示删除成功。
///
/// 数据构造（含关键数值的推导过程）：
///   seeded definitions         = 2 条，分别命名 "Delete Me" 与 "Keep Me"
///   delete target              = 1 条，验证只删目标记录
///   remaining definitions      = 1 条，验证非目标记录保留
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建真实 temp project store，并写入 2 条 definition
///   2. bind_project_store() 后调用 delete_definition(target_id)
///   3. 读取 state.definitions() 与 store.list()，检查删除结果
///
/// 预期结果：
///   - 正断言：delete_definition() 返回 true
///   - 正断言：内存与持久化都只剩下 "Keep Me"
///   - 负断言："Delete Me" 不应继续出现在任一列表中
#[test]
fn test_agent_definitions_state_delete_removes_definition() {
    let store = make_agent_definition_store("task-5-delete");
    let deleted = store
        .create(&NewAgentDefinition {
            name: "Delete Me".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "Delete workflow".to_string(),
        })
        .expect("test should seed deletable definition");
    store
        .create(&NewAgentDefinition {
            name: "Keep Me".to_string(),
            runtime_profile: AgentProfile::Claude,
            workflow_description: "Keep workflow".to_string(),
        })
        .expect("test should seed preserved definition");
    let mut state = AgentDefinitionsState::default();
    state.bind_project_store(store.clone());

    let did_delete = state.delete_definition(deleted.id);
    let in_memory = state.definitions().to_vec();
    let persisted = store.list().expect("store list should load remaining definitions");

    assert!(did_delete, "delete_definition should return true for an existing id");
    assert_eq!(
        in_memory.len(),
        1,
        "state definitions should contain exactly one record after deleting one of two definitions"
    );
    assert_eq!(
        in_memory[0].name, "Keep Me",
        "state definitions should preserve the non-deleted definition"
    );
    assert!(
        !in_memory.iter().any(|definition| definition.name == "Delete Me"),
        "state definitions should not retain the deleted definition"
    );
    assert_eq!(
        persisted.len(),
        1,
        "project store should contain exactly one record after deleting one of two definitions"
    );
    assert_eq!(
        persisted[0].name, "Keep Me",
        "project store should preserve the non-deleted definition"
    );
    assert!(
        !persisted.iter().any(|definition| definition.name == "Delete Me"),
        "project store should not retain the deleted definition"
    );
}

/// AS-10A: toggle_enabled(id) 应把启用状态同时写回内存与底层 store，避免 UI 状态切换后刷新回弹。
///
/// 数据构造（含关键数值的推导过程）：
///   seeded definitions        = 1 条，enabled = true
///   toggle operations         = 1 次，从 true -> false
///   expected persisted count  = 1 条，验证只更新状态不增删记录
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建真实 temp project store，并先写入 1 条 enabled = true 的 definition
///   2. bind_project_store() 后调用 toggle_enabled(id)
///   3. 读取 state.definitions() 与 store.list()，检查 enabled 是否都变为 false
///
/// 预期结果：
///   - 正断言：toggle_enabled(id) 返回 true
///   - 正断言：内存与持久化都只保留 1 条记录，且 enabled = false
///   - 负断言：内存与持久化都不应继续保留 enabled = true 的旧状态
#[test]
fn test_agent_definitions_state_toggle_enabled_persists_to_store() {
    let store = make_agent_definition_store("task-5-toggle-enabled");
    let seeded = store
        .create(&NewAgentDefinition {
            name: "Toggle Me".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "Toggle workflow".to_string(),
        })
        .expect("test should seed toggled definition");
    let mut state = AgentDefinitionsState::default();
    state.bind_project_store(store.clone());

    let did_toggle = state.toggle_enabled(seeded.id);
    let in_memory = state.definitions().to_vec();
    let persisted = store
        .list()
        .expect("store list should load definitions after toggle");

    assert!(
        did_toggle,
        "toggle_enabled should return true for an existing definition id"
    );
    assert_eq!(
        in_memory.len(),
        1,
        "state definitions should still contain one record after toggling enabled"
    );
    assert!(
        !in_memory[0].enabled,
        "state definitions should reflect the toggled disabled state after persistence"
    );
    assert_eq!(
        persisted.len(),
        1,
        "project store should still contain one record after toggling enabled"
    );
    assert!(
        !persisted[0].enabled,
        "project store should persist the toggled disabled state"
    );
    assert!(
        in_memory[0].enabled == persisted[0].enabled,
        "state definitions and project store should agree on the toggled enabled value"
    );
}

/// AS-11: 当草稿名称与同项目已有 definition 重名时，save_draft() 应失败并暴露可读错误消息，而不是底层 SQLite 文本。
///
/// 数据构造（含关键数值的推导过程）：
///   existing definition        = 1 条，name = "Duplicate Agent"
///   duplicate draft name       = "Duplicate Agent"，与现有记录完全相同
///   duplicate workflow         = "Another workflow"，长度 16 <= 2000
///   expected persisted count   = 1 条，验证失败不会写入第二条记录
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建真实 temp project store，并先写入 1 条现有 definition
///   2. bind_project_store() 后设置同名 draft，再调用 save_draft()
///   3. 读取 state.status()、state.definitions() 与 store.list()，确认失败路径
///
/// 预期结果：
///   - 正断言：save_draft() 返回 false
///   - 正断言：status 为 "Agent name already exists in this project"
///   - 正断言：内存与持久化都仍只有原始 1 条记录
///   - 负断言：错误消息不应泄露 "UNIQUE constraint failed" 这类底层数据库文案
#[test]
fn test_agent_definitions_state_duplicate_name_sets_readable_error() {
    let store = make_agent_definition_store("task-5-duplicate");
    store
        .create(&NewAgentDefinition {
            name: "Duplicate Agent".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "Original workflow".to_string(),
        })
        .expect("test should seed original definition");
    let mut state = AgentDefinitionsState::default();
    state.bind_project_store(store.clone());

    state.set_draft_name("Duplicate Agent");
    state.set_draft_runtime_profile(AgentProfile::Claude);
    state.set_draft_workflow_description("Another workflow");

    let did_save = state.save_draft();
    let in_memory = state.definitions().to_vec();
    let persisted = store
        .list()
        .expect("store list should load original definitions after failed save");
    let status = state.status().map(str::to_string);

    assert!(
        !did_save,
        "save_draft should return false when the draft name duplicates an existing definition"
    );
    assert_eq!(
        status.as_deref(),
        Some("Agent name already exists in this project"),
        "duplicate-name save should expose the readable project-level error message"
    );
    assert_eq!(
        in_memory.len(),
        1,
        "state definitions should remain unchanged after a duplicate-name save failure"
    );
    assert_eq!(
        persisted.len(),
        1,
        "project store should not create a second row after a duplicate-name save failure"
    );
    assert_eq!(
        persisted[0].name, "Duplicate Agent",
        "project store should retain only the original definition after duplicate-name rejection"
    );
    assert!(
        !status
            .as_deref()
            .unwrap_or_default()
            .contains("UNIQUE constraint failed"),
        "duplicate-name error should not leak raw SQLite UNIQUE constraint text"
    );
}
