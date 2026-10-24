use crate::{ProjectStore, StoredAgentConversationTail, StoredConversationAvailability};
use tempfile::tempdir;

fn sample_tail(definition_id: i64, session_id: &str, node_id: u64) -> StoredAgentConversationTail {
    StoredAgentConversationTail {
        definition_id,
        node_id,
        session_id: session_id.to_string(),
        agent_name: "Alan".to_string(),
        profile: "codex".to_string(),
        last_user_message: Some("比较 A/B".to_string()),
        last_agent_message: Some("建议选 A".to_string()),
        updated_at_unix: 1_700_000_000,
        availability: StoredConversationAvailability::Available,
        user_message_truncated: false,
        agent_message_truncated: false,
    }
}

/// 项目库 migration 必须创建 agent_conversation_tails 表。
///
/// 数据构造：临时 workspace → ProjectStore::migrate
/// 执行：PRAGMA table_info(agent_conversation_tails)
/// 预期：存在 definition_id 与 session_id 列；不得缺少主键列。
#[test]
fn project_migration_creates_agent_conversation_tails_table() {
    let root = tempdir().expect("temp workspace");
    let store = ProjectStore::for_database_paths(
        &crate::DatabasePaths::isolated_test(root.path()),
        root.path(),
    )
    .expect("project store");
    store.migrate().expect("migrate project db");

    let connection = crate::migrations::open_project_connection(store.path()).expect("open db");
    let mut statement = connection
        .prepare("PRAGMA table_info(agent_conversation_tails)")
        .expect("pragma");
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .expect("columns")
        .collect::<Result<Vec<_>, _>>()
        .expect("column names");

    assert!(
        columns.iter().any(|name| name == "definition_id"),
        "表应包含 definition_id"
    );
    assert!(
        columns.iter().any(|name| name == "session_id"),
        "表应包含 session_id"
    );
    assert!(
        columns.iter().any(|name| name == "availability"),
        "表应包含 availability"
    );
    assert!(!columns.is_empty(), "migration 后表结构不得为空");
}

/// 同 (definition_id, session_id) upsert 覆盖旧尾部；不同项目库互相隔离。
#[test]
fn upsert_tail_overwrites_same_definition_session_and_isolates_projects() {
    let root_a = tempdir().expect("project a");
    let root_b = tempdir().expect("project b");
    let store_a = ProjectStore::for_database_paths(
        &crate::DatabasePaths::isolated_test(root_a.path()),
        root_a.path(),
    )
    .expect("store a");
    let store_b = ProjectStore::for_database_paths(
        &crate::DatabasePaths::isolated_test(root_b.path()),
        root_b.path(),
    )
    .expect("store b");
    store_a.migrate().expect("migrate a");
    store_b.migrate().expect("migrate b");

    // Seed definition rows for FK.
    for store in [&store_a, &store_b] {
        store
            .agent_definition_store()
            .create(&storage_test_definition())
            .expect("create definition");
    }

    let tails_a = store_a.agent_conversation_tail_store();
    let tails_b = store_b.agent_conversation_tail_store();
    let mut first = sample_tail(1, "sess-1", 11);
    tails_a.upsert(&first).expect("first upsert");
    first.last_agent_message = Some("改为建议 B".to_string());
    first.updated_at_unix = 1_700_000_100;
    tails_a.upsert(&first).expect("overwrite upsert");
    tails_b
        .upsert(&sample_tail(1, "sess-1", 99))
        .expect("project b upsert");

    let loaded_a = tails_a
        .get(1, "sess-1")
        .expect("load a")
        .expect("tail a exists");
    let loaded_b = tails_b
        .get(1, "sess-1")
        .expect("load b")
        .expect("tail b exists");

    assert_eq!(
        loaded_a.last_agent_message.as_deref(),
        Some("改为建议 B"),
        "同 session 应覆盖旧完整回复"
    );
    assert_ne!(
        loaded_a.last_agent_message, loaded_b.last_agent_message,
        "不同项目库必须隔离"
    );
    assert_eq!(loaded_b.node_id, 99, "项目 B 应保留自己的 node_id");
}

/// 删除卡片/session 后对应尾部必须清理，不能再被续聊读取。
#[test]
fn delete_tail_on_session_end_or_card_removal() {
    let root = tempdir().expect("workspace");
    let store = ProjectStore::for_database_paths(
        &crate::DatabasePaths::isolated_test(root.path()),
        root.path(),
    )
    .expect("store");
    store.migrate().expect("migrate");
    store
        .agent_definition_store()
        .create(&storage_test_definition())
        .expect("definition");
    let tails = store.agent_conversation_tail_store();
    tails
        .upsert(&sample_tail(1, "sess-end", 42))
        .expect("upsert");

    assert!(
        tails.delete(1, "sess-end").expect("delete session"),
        "按 session 删除应成功"
    );
    assert!(
        tails.get(1, "sess-end").expect("get").is_none(),
        "删除后不得再读到尾部"
    );

    tails
        .upsert(&sample_tail(1, "sess-node", 42))
        .expect("upsert node tail");
    assert_eq!(
        tails.delete_for_node(42).expect("delete node"),
        1,
        "按 node 删除应清掉 1 条"
    );
    assert!(
        tails
            .get(1, "sess-node")
            .expect("get after node delete")
            .is_none(),
        "node 删除后不得残留"
    );
}

fn storage_test_definition() -> crate::NewAgentDefinition {
    crate::NewAgentDefinition {
        name: "Alan".to_string(),
        runtime_profile: canvas::AgentProfile::Codex,
        workflow_description: String::new(),
    }
}
