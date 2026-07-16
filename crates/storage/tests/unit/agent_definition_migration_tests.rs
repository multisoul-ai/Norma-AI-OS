use super::*;
use crate::migrations::open_project_connection;

/// 旧库迁移：正式名与内置语音别名冲突时，应保留两个正式名并停用冲突别名。
///
/// 数据构造（含关键数值的推导过程）：
///   legacy definitions = 2 条：Alan(id=1) 与 艾伦(id=2)
///   legacy schema       = 旧版 8 个业务列，不含 name_key/speech_alias
///   Alan builtin alias  = 艾伦，与第二条正式名精确冲突
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 直接构造升级前 agent_definitions 表并写入 Alan、艾伦；
///   2. open_project_connection 运行生产迁移，回填统一命名元数据；
///   3. 通过 AgentDefinitionStore 公开接口读取两条定义及语音别名。
///
/// 预期结果：
///   - 正断言：Alan 与 艾伦两个用户正式名都被保留；
///   - 正断言：冲突的 Alan speech_alias 被置空；
///   - 负断言：不得同时保留 Alan→艾伦 别名和 艾伦 正式名。
#[test]
fn test_legacy_migration_disables_builtin_alias_that_conflicts_with_formal_name() {
    let temp = tempfile::tempdir().expect("旧库冲突迁移测试应创建临时目录");
    let db_path = temp.path().join("workspace/.multisoul/project.sqlite");
    std::fs::create_dir_all(
        db_path
            .parent()
            .expect("project.sqlite 应存在 .multisoul 父目录"),
    )
    .expect("应创建旧库父目录");
    let legacy = rusqlite::Connection::open(&db_path).expect("应创建升级前 project.sqlite");
    legacy
        .execute_batch(
            "CREATE TABLE agent_definitions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                runtime_profile TEXT NOT NULL,
                workflow_description TEXT NOT NULL,
                enabled INTEGER NOT NULL,
                sort_order INTEGER NOT NULL,
                created_at_unix INTEGER NOT NULL,
                updated_at_unix INTEGER NOT NULL
             );
             CREATE UNIQUE INDEX idx_agent_definitions_name_nocase
             ON agent_definitions(name COLLATE NOCASE);
             INSERT INTO agent_definitions VALUES (1, 'Alan', 'codex', '', 1, 0, 1, 1);
             INSERT INTO agent_definitions VALUES (2, '艾伦', 'claude', '', 1, 1, 1, 1);",
        )
        .expect("应写入可由旧版唯一索引接受的 Alan/艾伦 两条定义");
    drop(legacy);

    open_project_connection(&db_path).expect("生产迁移应处理跨命名空间冲突而不是拒绝旧库");
    let definitions = AgentDefinitionStore::new(db_path)
        .list()
        .expect("迁移后应通过公开接口读取 Agent 定义");

    assert_eq!(definitions.len(), 2, "迁移应保留两个既有正式名");
    assert_eq!(definitions[0].name, "Alan", "第一条正式名 Alan 不应被改写");
    assert_eq!(definitions[1].name, "艾伦", "第二条正式名 艾伦 不应被删除");
    assert!(
        definitions[0].speech_alias.is_none(),
        "艾伦 已被正式名占用时，Alan 的内置语音别名必须停用"
    );
    assert!(
        definitions[1].speech_alias.is_none(),
        "自定义中文正式名不应被迁移生成额外语音别名"
    );
    assert_ne!(
        definitions[0].speech_alias.as_deref(),
        Some(definitions[1].name.as_str()),
        "迁移后不得让 Alan 的语音别名继续与 艾伦 正式名冲突"
    );
}
