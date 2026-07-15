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
///   初始 agent = 1 条：CodeReviewer / Codex / "Review code correctness, performance, and tests."
///   update 后 name = "ReviewLead"
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
            name: "CodeReviewer".to_string(),
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
            name: "ReviewLead".to_string(),
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
    assert_eq!(updated.name, "ReviewLead", "update 后名称应改为 ReviewLead");
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

/// AD-5：Agent 定义持久化应让正式名称与预设语音别名共享唯一命名空间。
///
/// 数据构造（含关键数值的推导过程）：
///   first = Alan / Codex / 空 Workflow Description
///   conflicting = 艾伦 / Claude / 空 Workflow Description
///   Alan 的名称键数量 = 2：正式键 alan + 语音别名键 艾伦
///   冲突写入前记录数 = 1，拒绝后记录数仍应 = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. create("Alan") → 保存正式名，并登记预设语音别名艾伦
///   2. create("艾伦") → 预检命中 Alan 的语音别名占用，返回 typed conflict
///   3. list() → 确认失败写入没有产生部分记录
///
/// 预期结果：
///   - 正断言：首条 Alan 创建成功且空 Workflow Description 被保留
///   - 正断言：第二条返回 requested=艾伦、existing=Alan 的名称冲突
///   - 负断言：列表中不应存在名为艾伦的第二条定义
#[test]
fn test_agent_definition_store_rejects_cross_namespace_speech_alias_conflict() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let store = AgentDefinitionStore::new(temp.path().join("project.sqlite"));

    let alan = store
        .create(&NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: String::new(),
        })
        .expect("Alan 应成功写入并占用正式名与预设语音别名");
    let conflicting = store.create(&NewAgentDefinition {
        name: "艾伦".to_string(),
        runtime_profile: AgentProfile::Claude,
        workflow_description: String::new(),
    });
    let definitions = store.list().expect("名称冲突后 list 仍应正常读取既有定义");

    assert_eq!(
        alan.workflow_description, "",
        "Workflow Description 首版可选，空文本应原样持久化"
    );
    match conflicting {
        Err(StorageError::AgentNameConflict {
            requested,
            existing,
        }) => {
            assert_eq!(requested, "艾伦", "冲突错误应回传用户请求的中文名称");
            assert_eq!(existing, "Alan", "冲突错误应指出占用语音别名的既有 Alan");
        }
        Err(other) => panic!("艾伦应返回 AgentNameConflict，实际得到 {other:?}"),
        Ok(value) => panic!("艾伦不应绕过 Alan 的语音别名占用，实际创建了 {value:?}"),
    }
    assert_eq!(
        definitions.len(),
        1,
        "冲突写入应零副作用，数据库仍只能包含 1 条 Alan 定义"
    );
    assert_eq!(
        definitions[0].name, "Alan",
        "冲突拒绝后保留的唯一记录必须是先创建的 Alan"
    );
    assert!(
        !definitions
            .iter()
            .any(|definition| definition.name == "艾伦"),
        "列表中不应出现被拒绝的中文名称艾伦"
    );
}

/// AD-4: update() 遇到不存在的 id 时，必须返回 typed storage error，不能伪造成功写入。
///
/// 数据构造（含关键数值的推导过程）：
///   existing rows = 0 条，确保要更新的 id=4242 在 project DB 中不存在
///   updated payload = "MissingReviewer" / Claude / "Should not be persisted."
///   target id = 4242，选取一个远离 AUTOINCREMENT 初始值的固定整数，避免和自动生成主键碰撞
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 直接调用 update(id=4242) → 触发 WHERE id = 4242 的 UPDATE
///   2. SQLite 返回 0 rows changed → 调用方应收到错误，而不是 synthetic AgentDefinition
///
/// 预期结果：
///   - 正断言：update() 返回 Err
///   - 正断言：错误类型应为 AgentDefinitionNotFound(4242) 以便上层处理缺失记录
///   - 负断言：不能返回看似成功的 AgentDefinition，因为那会掩盖未持久化的状态
#[test]
fn test_agent_definition_store_update_missing_id_returns_typed_error() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let store = AgentDefinitionStore::new(temp.path().join("project.sqlite"));

    let result = store.update(&AgentDefinitionUpdate {
        id: 4242,
        name: "MissingReviewer".to_string(),
        runtime_profile: AgentProfile::Claude,
        workflow_description: "Should not be persisted.".to_string(),
        enabled: true,
        sort_order: 7,
    });

    match result {
        Err(StorageError::AgentDefinitionNotFound(id)) => {
            assert_eq!(
                id, 4242,
                "missing-id 错误必须回传原始 id 供调用方定位缺失记录"
            );
        }
        Err(other) => panic!("update() 应返回 AgentDefinitionNotFound(4242)，实际得到 {other:?}"),
        Ok(value) => panic!(
            "update() 不应伪造成功 AgentDefinition；实际错误地返回了 {:?}",
            value
        ),
    }
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

/// AD-6：定义仓库本身必须拒绝非法呼叫名，不能只依赖 Settings 或语音入口校验。
///
/// 数据构造（含关键数值的推导过程）：
///   invalid name = "Alan 2"，共 6 个字符但包含 1 个空格和 1 个数字
///   合法英文规则 = 2–16 个 ASCII 字母，因此该名称不满足字符集合约束
///   初始 definitions = 0，失败后仍应 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 直接调用 AgentDefinitionStore::create，绕过 app 层输入校验
///   2. 仓库在打开写事务前解析 AgentName
///   3. list() 检查数据库未产生部分写入
///
/// 预期结果：
///   - 正断言：create 返回 InvalidAgentName("Alan 2")
///   - 正断言：失败后 definitions 仍为空
///   - 负断言：非法名称不得被持久化为定义
#[test]
fn test_agent_definition_store_rejects_invalid_name_at_storage_boundary() {
    let temp = tempfile::tempdir().expect("测试应创建临时目录");
    let store = AgentDefinitionStore::new(temp.path().join("project.sqlite"));

    let result = store.create(&NewAgentDefinition {
        name: "Alan 2".to_string(),
        runtime_profile: AgentProfile::Codex,
        workflow_description: String::new(),
    });
    let definitions = store.list().expect("非法名称拒绝后仍应能读取定义列表");

    match result {
        Err(StorageError::InvalidAgentName(name)) => {
            assert_eq!(name, "Alan 2", "typed error 应保留原始非法名称 Alan 2");
        }
        Err(other) => panic!("非法名称应返回 InvalidAgentName，实际得到 {other:?}"),
        Ok(definition) => panic!("非法名称不应创建成功，实际得到 {definition:?}"),
    }
    assert_eq!(definitions.len(), 0, "非法名称失败后定义列表必须仍为 0 条");
    assert!(
        !definitions
            .iter()
            .any(|definition| definition.name == "Alan 2"),
        "非法名称 Alan 2 不得出现在持久化定义中"
    );
}

/// AD-7：删除已关闭 Agent 时应同时清理其委派记录并释放名称。
///
/// 数据构造（含关键数值的推导过程）：
///   definitions = 1 条 Alan；dispatch tasks = 1 条 Done
///   active cards = 0，调用方已满足“先关闭卡片”前置条件
///   删除事务涉及 2 张表：agent_dispatch_tasks + agent_definitions
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Alan 定义并写入一条已完成委派记录
///   2. delete(Alan.id) 在同一事务先清理关联记录，再删除定义
///   3. 再次创建同名 Alan，验证名称已释放
///
/// 预期结果：
///   - 正断言：delete 返回 true，定义和委派记录都为空
///   - 正断言：同名 Alan 可以再次创建
///   - 负断言：历史外键不得阻止删除或继续占用名称
#[test]
fn test_agent_definition_store_delete_clears_dispatch_history_and_releases_name() {
    let temp = tempfile::tempdir().expect("测试应创建临时目录");
    let path = temp.path().join("project.sqlite");
    let store = AgentDefinitionStore::new(path.clone());
    let alan = store
        .create(&NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: String::new(),
        })
        .expect("测试应创建 Alan 定义");
    crate::AgentDispatchTaskStore::new(path.clone())
        .create(&crate::NewAgentDispatchTask {
            definition_id: alan.id,
            node_id: 77,
            original_instruction: "Alan，检查测试".to_string(),
            task_text: "检查测试".to_string(),
            state: crate::AgentDispatchTaskState::Done,
            tracking: crate::AgentDispatchTracking::Managed,
        })
        .expect("测试应写入一条已完成委派记录");

    let deleted = store
        .delete(alan.id)
        .expect("历史委派不应阻止删除已关闭 Agent");
    let definitions = store.list().expect("删除后应能读取空定义列表");
    let tasks = crate::AgentDispatchTaskStore::new(path)
        .list()
        .expect("删除后应能读取空委派列表");
    let recreated = store
        .create(&NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: AgentProfile::Claude,
            workflow_description: String::new(),
        })
        .expect("删除定义后 Alan 名称应立即释放并可复用");

    assert!(deleted, "存在的 Alan 定义应返回 deleted=true");
    assert_eq!(definitions.len(), 0, "删除事务后旧定义列表必须为空");
    assert_eq!(tasks.len(), 0, "删除事务必须清理旧 Alan 的委派记录");
    assert_eq!(
        recreated.name, "Alan",
        "释放后的名称应可用于新 Claude Agent"
    );
    assert_ne!(
        recreated.id, alan.id,
        "重新创建的 Alan 必须获得新的稳定定义 id"
    );
}
