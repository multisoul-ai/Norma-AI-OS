use super::*;
use canvas::AgentProfile;
use gpui::{Keystroke, Modifiers};
use storage::{NewAgentDefinition, ProjectStore};

fn key(key: &str, key_char: Option<&str>) -> Keystroke {
    Keystroke {
        modifiers: Modifiers::default(),
        key: key.to_string(),
        key_char: key_char.map(str::to_string),
    }
}

/// AS-2A: bind_project_store() 切换到另一个 project store 时，必须清空前一个项目留下的 draft/edit buffer，避免把 Project A 的草稿误保存进 Project B。
///
/// 数据构造（含关键数值的推导过程）：
///   store A seeded rows        = 1 条，name = "Alpha Agent"
///   store B seeded rows        = 1 条，name = "Beta Agent"
///   overlapping ids           = A/B 首条记录都预计为 id=1，用于暴露“按旧 draft.id 更新到新 store”风险
///   pending visible name      = "Leaked Agent Name"，长度 17 <= 80
///   pending visible workflow  = "Leaked workflow from A"，长度 22 <= 2000
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 store A / store B 各创建 1 条 definition，并 bind 到 A
///   2. edit_definition(a.id) 后进入 Name / WorkflowDescription 编辑态，写入尚未提交的 Project A 可见草稿
///   3. 直接 bind_project_store(store_b.clone()) 切到 Project B，再调用 save_draft()
///   4. 分别读取 state.definitions()、store_a.list()、store_b.list() 验证切换后的保存行为
///
/// 预期结果：
///   - 正断言：切到 store B 后 draft 自动加载 Project B 的首条 definition
///   - 正断言：save_draft() 返回 true 且只保存 Project B 自己的详情
///   - 正断言：state.definitions() 只显示 Project B 原始记录
///   - 正断言：store A 原始记录保持不变，说明旧项目数据没有被误改
///   - 正断言：store B 原始记录保持不变，说明没有把 Project A 草稿误写进新项目
///   - 负断言："Leaked Agent Name" 与 "Leaked workflow from A" 不应出现在任一 store
#[test]
fn test_agent_definitions_state_bind_project_store_clears_stale_draft_before_save() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let workspace_a = temp.path().join("switch-a");
    let workspace_b = temp.path().join("switch-b");
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

    let store_a = project_a.agent_definition_store();
    let store_b = project_b.agent_definition_store();
    let alpha = store_a
        .create(&NewAgentDefinition {
            name: "Alpha Agent".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "Alpha workflow".to_string(),
        })
        .expect("test should seed project A definition");
    let beta = store_b
        .create(&NewAgentDefinition {
            name: "Beta Agent".to_string(),
            runtime_profile: AgentProfile::Claude,
            workflow_description: "Beta workflow".to_string(),
        })
        .expect("test should seed project B definition");
    let mut state = AgentDefinitionsState::default();
    state.bind_project_store(store_a.clone());

    let did_edit = state.edit_definition(alpha.id);
    state.begin_edit(AgentDefinitionField::Name);
    for _ in 0.."Alpha Agent".chars().count() {
        state.handle_key_down(&key("backspace", None));
    }
    for ch in "Leaked Agent Name".chars() {
        let keystroke = if ch == ' ' {
            key("space", Some(" "))
        } else {
            let text = ch.to_string();
            key(&text, Some(&text))
        };
        state.handle_key_down(&keystroke);
    }
    state.begin_edit(AgentDefinitionField::WorkflowDescription);
    for _ in 0.."Alpha workflow".chars().count() {
        state.handle_key_down(&key("backspace", None));
    }
    for ch in "Leaked workflow from A".chars() {
        let keystroke = if ch == ' ' {
            key("space", Some(" "))
        } else {
            let text = ch.to_string();
            key(&text, Some(&text))
        };
        state.handle_key_down(&keystroke);
    }

    state.bind_project_store(store_b.clone());

    let selected_after_bind = state.selected_definition_id();
    let draft_after_bind = state.draft().clone();
    let did_save = state.save_draft();
    let in_memory = state.definitions().to_vec();
    let persisted_a = store_a
        .list()
        .expect("project A store list should load definitions after switching away");
    let persisted_b = store_b
        .list()
        .expect("project B store list should load definitions after switching in");

    assert!(
        did_edit,
        "edit_definition should return true so the test starts from a real Project A draft"
    );
    assert_eq!(
        selected_after_bind,
        Some(beta.id),
        "切换到 Project B 后应自动选中 Project B 的首条 definition，而不是保留 Project A selected id"
    );
    assert_eq!(
        draft_after_bind.name, "Beta Agent",
        "切换到 Project B 后 draft 应加载 Project B 名称，避免 Project A pending edit 泄漏"
    );
    assert!(
        did_save,
        "切换到 Project B 后 save_draft 应保存当前 Project B 详情，而不是失败或写入 Project A 草稿"
    );
    assert_eq!(
        in_memory.len(),
        1,
        "state definitions should show only the single Project B definition after rebinding the store"
    );
    assert_eq!(
        in_memory[0].id, beta.id,
        "state definitions should stay scoped to Project B after rebinding the store"
    );
    assert_eq!(
        in_memory[0].name, "Beta Agent",
        "state definitions should preserve Project B's original agent name after the failed save"
    );
    assert_eq!(
        persisted_a.len(),
        1,
        "project A should still contain exactly one definition after switching stores"
    );
    assert_eq!(
        persisted_a[0].id, alpha.id,
        "project A should still contain the original definition id after switching stores"
    );
    assert_eq!(
        persisted_a[0].name, "Alpha Agent",
        "project A should retain its original agent name after switching stores"
    );
    assert_eq!(
        persisted_a[0].workflow_description, "Alpha workflow",
        "project A should retain its original workflow after switching stores"
    );
    assert_eq!(
        persisted_b.len(),
        1,
        "project B should still contain exactly one definition after the failed save"
    );
    assert_eq!(
        persisted_b[0].id, beta.id,
        "project B should still contain the original definition id after switching stores"
    );
    assert_eq!(
        persisted_b[0].name, "Beta Agent",
        "project B should retain its original agent name instead of Project A's stale draft"
    );
    assert_eq!(
        persisted_b[0].workflow_description, "Beta workflow",
        "project B should retain its original workflow instead of Project A's stale draft"
    );
    assert!(
        !persisted_a
            .iter()
            .any(|definition| definition.name == "Leaked Agent Name"),
        "project A should not persist the leaked draft name after switching stores"
    );
    assert!(
        !persisted_b
            .iter()
            .any(|definition| definition.name == "Leaked Agent Name"),
        "project B should not persist the leaked draft name after switching stores"
    );
    assert!(
        !persisted_a
            .iter()
            .any(|definition| definition.workflow_description == "Leaked workflow from A"),
        "project A should not persist the leaked draft workflow after switching stores"
    );
    assert!(
        !persisted_b
            .iter()
            .any(|definition| definition.workflow_description == "Leaked workflow from A"),
        "project B should not persist the leaked draft workflow after switching stores"
    );
}
