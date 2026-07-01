use super::*;
use crate::migrations::open_project_connection;
use crate::StorageError;
use canvas::AgentProfile;

/// Agent definitions migration: project DB 应创建 agent_definitions 表和大小写不敏感唯一索引。
///
/// 数据构造（含关键数值的推导过程）：
///   project.sqlite 位于 temp/workspace/.multisoul/project.sqlite
///   agent_definitions 需要 8 个业务列：id/name/runtime_profile/workflow_description/enabled/sort_order/created_at_unix/updated_at_unix
///   unique index = idx_agent_definitions_name_nocase，用于阻止 "Reviewer" 与 "reviewer" 同项目共存
///
/// 执行过程（逐步说明系统如何处理）：
///   1. open_project_connection(project.sqlite) 触发 migrate_project_db
///   2. PRAGMA table_info(agent_definitions) 读取列名
///   3. PRAGMA index_list(agent_definitions) 读取索引名
///
/// 预期结果：
///   - 正断言：agent_definitions 表包含 name/runtime_profile/workflow_description/enabled
///   - 正断言：存在 idx_agent_definitions_name_nocase
///   - 负断言：不应缺少 workflow_description，否则无法给未来语音规划提供上下文
#[test]
fn test_agent_definitions_migration_creates_table_and_unique_index() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let db_path = temp
        .path()
        .join("workspace")
        .join(".multisoul")
        .join("project.sqlite");
    let connection = open_project_connection(&db_path).expect("project db should migrate");

    let mut columns_stmt = connection
        .prepare("PRAGMA table_info(agent_definitions)")
        .expect("table_info should prepare");
    let columns = columns_stmt
        .query_map([], |row| row.get::<_, String>(1))
        .expect("table_info should query")
        .collect::<Result<Vec<_>, _>>()
        .expect("columns should collect");

    let mut index_stmt = connection
        .prepare("PRAGMA index_list(agent_definitions)")
        .expect("index_list should prepare");
    let indexes = index_stmt
        .query_map([], |row| row.get::<_, String>(1))
        .expect("index_list should query")
        .collect::<Result<Vec<_>, _>>()
        .expect("indexes should collect");

    assert!(
        columns.iter().any(|column| column == "name"),
        "agent_definitions 必须包含 name 列作为 Settings 和语音匹配的显示名"
    );
    assert!(
        columns.iter().any(|column| column == "runtime_profile"),
        "agent_definitions 必须包含 runtime_profile 列以复用 Claude/Codex/OpenCode runtime"
    );
    assert!(
        columns
            .iter()
            .any(|column| column == "workflow_description"),
        "agent_definitions 必须包含 workflow_description 列供未来语音 planner 使用"
    );
    assert!(
        columns.iter().any(|column| column == "enabled"),
        "agent_definitions 必须包含 enabled 列以便未来语音上下文过滤禁用 Agent"
    );
    assert!(
        indexes
            .iter()
            .any(|index| index == "idx_agent_definitions_name_nocase"),
        "agent_definitions 必须有大小写不敏感 name 唯一索引"
    );
    assert!(
        !columns.is_empty(),
        "agent_definitions 表不应缺失，否则 PRAGMA table_info 会返回空列表"
    );
}

/// AD-1: AgentDefinitionStore 应支持 create/list/update/delete 的完整往返。
///
/// 数据构造（含关键数值的推导过程）：
///   初始 agent = 1 条：Code Reviewer / Codex / "Review code correctness, performance, and tests."
///   update 后 name = "Code Review Lead"
///   update 后 runtime_profile = Claude
///   update 后 enabled = false，sort_order = 9
///
/// 执行过程（逐步说明系统如何处理）：
///   1. create(initial) → SQLite 插入默认 enabled=1、sort_order=0 的 agent_definition
///   2. list() → 读取唯一一条记录
///   3. update(updated) → 覆盖名称/profile/workflow/enabled/sort_order
///   4. delete(id) → 删除该记录
///   5. delete(id) 再执行一次 → 验证幂等 false
///   6. final list() → 验证表已清空
///
/// 预期结果：
///   - 正断言：created id > 0，且首次 list() 恰好返回 1 条
///   - 正断言：update 后字段全部变更，delete 第一次为 true、第二次为 false
///   - 负断言：final list() 不应残留旧 agent_definition
#[test]
fn test_agent_definition_store_create_list_update_delete_round_trips() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let store = AgentDefinitionStore::new(temp.path().join("project.sqlite"));
    let created = store
        .create(&NewAgentDefinition {
            name: "Code Reviewer".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "Review code correctness, performance, and tests.".to_string(),
        })
        .expect("create 应写入首条 agent_definition");

    let listed = store
        .list()
        .expect("首次 list 应读取创建后的 agent_definition");
    let updated = store
        .update(&AgentDefinitionUpdate {
            id: created.id,
            name: "Code Review Lead".to_string(),
            runtime_profile: AgentProfile::Claude,
            workflow_description: "Lead review workflow, triage risk, and request follow-up fixes."
                .to_string(),
            enabled: false,
            sort_order: 9,
        })
        .expect("update 应覆盖 agent_definition 字段");
    let first_delete = store
        .delete(created.id)
        .expect("第一次 delete 应删除已有 agent_definition");
    let second_delete = store
        .delete(created.id)
        .expect("第二次 delete 应返回 false 而不是报错");
    let final_list = store.list().expect("删除后 list 应可读取空结果");

    assert!(
        created.id > 0,
        "create 返回的 agent_definition id 必须大于 0"
    );
    assert_eq!(
        listed.len(),
        1,
        "首次 list() 应只返回刚创建的 1 条 agent_definition"
    );
    assert_eq!(
        listed[0], created,
        "首次 list() 读回的记录应与 create 返回值完全一致"
    );
    assert_eq!(
        updated.name, "Code Review Lead",
        "update 后名称应改为 Code Review Lead"
    );
    assert_eq!(
        updated.runtime_profile,
        AgentProfile::Claude,
        "update 后 runtime_profile 应改为 Claude"
    );
    assert_eq!(
        updated.workflow_description,
        "Lead review workflow, triage risk, and request follow-up fixes.",
        "update 后 workflow_description 应读回最新内容"
    );
    assert!(
        !updated.enabled,
        "update 后 enabled 应为 false，供未来禁用 Agent 的过滤逻辑使用"
    );
    assert_eq!(updated.sort_order, 9, "update 后 sort_order 应改为 9");
    assert!(
        first_delete,
        "第一次 delete 应返回 true，表示确实删除了 agent_definition"
    );
    assert!(
        !second_delete,
        "第二次 delete 应返回 false，表示记录已不存在"
    );
    assert!(
        final_list.is_empty(),
        "delete 后 final list() 不应残留任何 agent_definition"
    );
}

/// AD-2: agent_definitions 的 name 唯一约束必须大小写不敏感，避免 Settings/语音选择出现重复项。
///
/// 数据构造（含关键数值的推导过程）：
///   first name = "Reviewer"
///   duplicate name = "reviewer"，仅大小写不同
///   两条记录共享同一项目库，因此 unique index 应拒绝第二次 create
///
/// 执行过程（逐步说明系统如何处理）：
///   1. create("Reviewer") → 首条记录成功写入
///   2. create("reviewer") → 命中 idx_agent_definitions_name_nocase
///
/// 预期结果：
///   - 正断言：第一条 create 成功
///   - 正断言：第二条 create 返回 Err
///   - 负断言：不允许 "Reviewer" 与 "reviewer" 同项目共存
#[test]
fn test_agent_definition_store_rejects_case_insensitive_duplicate_names() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let store = AgentDefinitionStore::new(temp.path().join("project.sqlite"));

    store
        .create(&NewAgentDefinition {
            name: "Reviewer".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "Review correctness.".to_string(),
        })
        .expect("首条 Reviewer 应成功写入");
    let duplicate_result = store.create(&NewAgentDefinition {
        name: "reviewer".to_string(),
        runtime_profile: AgentProfile::Claude,
        workflow_description: "Review via Claude.".to_string(),
    });

    assert!(
        duplicate_result.is_err(),
        "大小写不同但语义相同的 reviewer 不应重复创建成功"
    );
}

/// AD-3: list() 遇到非法 runtime_profile 脏数据时，应返回 typed StorageError 供调用方上报迁移问题。
///
/// 数据构造（含关键数值的推导过程）：
///   valid business columns = 7 个写入值：name/runtime_profile/workflow_description/enabled/sort_order/created_at_unix/updated_at_unix
///   runtime_profile = "bad-runtime"，故意绕过 store 校验直接插入 SQLite
///   timestamps = 1/1，满足 NOT NULL 约束且不影响 profile 解码路径
///
/// 执行过程（逐步说明系统如何处理）：
///   1. open_project_connection(project.sqlite) → 确保 migration 已创建 agent_definitions
///   2. execute INSERT ... runtime_profile='bad-runtime' → 制造脏数据
///   3. AgentDefinitionStore::list() → 读取并尝试 decode runtime_profile
///
/// 预期结果：
///   - 正断言：list() 返回 StorageError::InvalidAgentProfile("bad-runtime")
///   - 负断言：不应把 bad-runtime 静默映射成 Claude/Codex/OpenCode 之一
#[test]
fn test_agent_definition_store_invalid_runtime_profile_returns_typed_error() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let db_path = temp.path().join("project.sqlite");
    let store = AgentDefinitionStore::new(db_path.clone());
    let connection = open_project_connection(&db_path).expect("project db should migrate");

    connection
        .execute(
            "INSERT INTO agent_definitions \
             (name, runtime_profile, workflow_description, enabled, sort_order, created_at_unix, updated_at_unix) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                "Broken Reviewer",
                "bad-runtime",
                "Corrupted row for migration guardrail.",
                1,
                0,
                1,
                1,
            ],
        )
        .expect("test setup should insert bad-runtime row directly");

    let result = store.list();

    match result {
        Err(StorageError::InvalidAgentProfile(profile)) => {
            assert_eq!(
                profile, "bad-runtime",
                "typed error 应回传原始 bad-runtime 值供调用方诊断脏数据"
            );
        }
        Err(other) => {
            panic!("list() 应返回 InvalidAgentProfile(\"bad-runtime\")，实际得到 {other:?}")
        }
        Ok(definitions) => panic!(
            "list() 不应静默接受 bad-runtime；实际错误地返回了 {:?}",
            definitions
        ),
    }
}
