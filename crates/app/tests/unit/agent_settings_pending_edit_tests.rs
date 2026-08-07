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

/// AS-9A: save_draft() 遇到正在编辑中的字段时，必须先提交可见 edit_buffer，再做校验和持久化，避免 Save 丢失用户眼前已输入但未按 Enter 的文本。
///
/// 数据构造（含关键数值的推导过程）：
///   seeded definitions        = 1 条，name = "Original"
///   visible pending name      = "Visible Draft Name"，长度 18 <= 80
///   stored draft name before save = "Original"，验证 save 前 self.draft 仍是旧值
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
///   - 负断言：旧名称 "Original" 不应继续留在内存或持久化列表中
#[test]
fn test_agent_definitions_state_save_draft_commits_pending_active_edit_buffer() {
    let store = make_agent_definition_store("task-5-pending-edit-save");
    let original = store
        .create(&NewAgentDefinition {
            name: "Original".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "Original workflow".to_string(),
        })
        .expect("test should seed original definition");
    let mut state = AgentDefinitionsState::default();
    state.bind_project_store(store.clone());

    let did_edit = state.edit_definition(original.id);
    state.begin_edit(AgentDefinitionField::Name);
    for _ in 0.."Original".chars().count() {
        state.handle_key_down(&key("backspace", None));
    }
    for ch in "VisibleDraft".chars() {
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
        in_memory[0].name, "VisibleDraft",
        "state definitions should persist the visible edit buffer text instead of the stale draft name"
    );
    assert!(
        !in_memory
            .iter()
            .any(|definition| definition.name == "Original"),
        "state definitions should not retain the stale pre-edit name after save"
    );
    assert_eq!(
        persisted.len(),
        1,
        "project store should still contain one record after saving a pending in-place edit"
    );
    assert_eq!(
        persisted[0].name, "VisibleDraft",
        "project store should persist the visible edit buffer text instead of the stale draft name"
    );
    assert!(
        !persisted
            .iter()
            .any(|definition| definition.name == "Original"),
        "project store should not retain the stale pre-edit name after save"
    );
}

/// AS-10: delete_definition() 应同时删除内存与底层 store 的 definition，并返回 true 表示删除成功。
///
/// 数据构造（含关键数值的推导过程）：
///   seeded definitions         = 2 条，分别命名 "Delete" 与 "Keep"
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
///   - 正断言：内存与持久化都只剩下 "Keep"
///   - 负断言："Delete" 不应继续出现在任一列表中
#[test]
fn test_agent_definitions_state_delete_removes_definition() {
    let store = make_agent_definition_store("task-5-delete");
    let deleted = store
        .create(&NewAgentDefinition {
            name: "Delete".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "Delete workflow".to_string(),
        })
        .expect("test should seed deletable definition");
    store
        .create(&NewAgentDefinition {
            name: "Keep".to_string(),
            runtime_profile: AgentProfile::Claude,
            workflow_description: "Keep workflow".to_string(),
        })
        .expect("test should seed preserved definition");
    let mut state = AgentDefinitionsState::default();
    state.bind_project_store(store.clone());

    let did_delete = state.delete_definition(deleted.id);
    let in_memory = state.definitions().to_vec();
    let persisted = store
        .list()
        .expect("store list should load remaining definitions");

    assert!(
        did_delete,
        "delete_definition should return true for an existing id"
    );
    assert_eq!(
        in_memory.len(),
        1,
        "state definitions should contain exactly one record after deleting one of two definitions"
    );
    assert_eq!(
        in_memory[0].name, "Keep",
        "state definitions should preserve the non-deleted definition"
    );
    assert!(
        !in_memory
            .iter()
            .any(|definition| definition.name == "Delete"),
        "state definitions should not retain the deleted definition"
    );
    assert_eq!(
        persisted.len(),
        1,
        "project store should contain exactly one record after deleting one of two definitions"
    );
    assert_eq!(
        persisted[0].name, "Keep",
        "project store should preserve the non-deleted definition"
    );
    assert!(
        !persisted
            .iter()
            .any(|definition| definition.name == "Delete"),
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
            name: "Toggle".to_string(),
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
