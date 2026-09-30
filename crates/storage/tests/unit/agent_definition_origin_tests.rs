use super::*;
use canvas::AgentProfile;

/// AD-ORIGIN-1：显式创建与快速 Card 创建必须持久化不同的 Definition 生命周期来源。
///
/// 数据构造（含关键数值的推导过程）：
///   explicit definitions = 1 条：Alan / Codex，由 Settings 等显式入口创建
///   quick definitions    = 1 条：Bella / Claude，由快捷键或 `+ Agent` 快速入口创建
///   Total                = 1 + 1 = 2 条
///
/// 执行过程（逐步说明系统如何处理）：
///   1. `store.create(Alan)` 走长期角色创建接口。
///   2. `store.create_quick(Bella)` 走临时 Card Definition 创建接口。
///   3. `store.list()` 重新从 SQLite 读取两条记录，避免只验证返回值缓存。
///
/// 预期结果：
///   - 正断言：Alan 的来源为 Explicit，关闭 Card 后应保留。
///   - 正断言：Bella 的来源为 QuickCard，关闭 Card 后允许清理。
///   - 负断言：两种创建入口不得都落成相同来源，否则删除逻辑无法安全区分。
#[test]
fn test_agent_definition_store_distinguishes_explicit_and_quick_card_origins() {
    let temp = tempfile::tempdir().expect("Definition 来源测试应创建临时目录");
    let store = AgentDefinitionStore::new(temp.path().join("project.sqlite"));
    let explicit = store
        .create(&NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "Long-lived reviewer".to_string(),
        })
        .expect("显式入口应创建长期 Agent Definition");
    let quick = store
        .create_quick(&NewAgentDefinition {
            name: "Bella".to_string(),
            runtime_profile: AgentProfile::Claude,
            workflow_description: String::new(),
        })
        .expect("快速入口应创建临时 Card Definition");

    let listed = store.list().expect("应从 SQLite 重新读取两条 Definition");
    let listed_explicit = listed
        .iter()
        .find(|definition| definition.id == explicit.id)
        .expect("list 应包含显式创建的 Alan");
    let listed_quick = listed
        .iter()
        .find(|definition| definition.id == quick.id)
        .expect("list 应包含快速创建的 Bella");

    assert_eq!(
        listed.len(),
        2,
        "两种入口各创建 1 条 Definition 后应恰好有 2 条记录"
    );
    assert_eq!(
        listed_explicit.origin,
        AgentDefinitionOrigin::Explicit,
        "显式创建的 Alan 必须标记为 Explicit"
    );
    assert_eq!(
        listed_quick.origin,
        AgentDefinitionOrigin::QuickCard,
        "快速创建的 Bella 必须标记为 QuickCard"
    );
    assert_ne!(
        listed_explicit.origin, listed_quick.origin,
        "显式角色与快速 Card Definition 不得共享相同生命周期来源"
    );
}

/// AD-ORIGIN-2：生命周期清理只能删除 QuickCard Definition，必须原子保护 Explicit 长期角色。
///
/// 数据构造（含关键数值的推导过程）：
///   explicit definitions = 1 条：Alan
///   quick definitions    = 1 条：Bella
///   初始总数             = 1 + 1 = 2 条
///   清理请求             = Alan 1 次 + Bella 1 次
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别创建 Explicit Alan 与 QuickCard Bella。
///   2. 对 Alan 调用 `delete_if_quick`，来源不匹配，应零副作用返回 false。
///   3. 对 Bella 调用 `delete_if_quick`，来源匹配，应删除并返回 true。
///   4. 重新 `list()`，验证最终仅保留 Alan。
///
/// 预期结果：
///   - 正断言：QuickCard Bella 被清理，最终仅剩 1 条 Explicit Alan。
///   - 正断言：Explicit Alan 仍可读取，长期角色生命周期不受 Card 删除影响。
///   - 负断言：对 Explicit Alan 的清理请求不得返回 true 或删除记录。
///   - 负断言：最终列表不得残留 QuickCard Bella。
#[test]
fn test_delete_if_quick_only_removes_quick_card_definitions() {
    let temp = tempfile::tempdir().expect("Definition 清理测试应创建临时目录");
    let store = AgentDefinitionStore::new(temp.path().join("project.sqlite"));
    let explicit = store
        .create(&NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "Long-lived reviewer".to_string(),
        })
        .expect("应创建 Explicit Alan");
    let quick = store
        .create_quick(&NewAgentDefinition {
            name: "Bella".to_string(),
            runtime_profile: AgentProfile::Claude,
            workflow_description: String::new(),
        })
        .expect("应创建 QuickCard Bella");

    let explicit_deleted = store
        .delete_if_quick(explicit.id)
        .expect("清理 Explicit Alan 应安全返回结果");
    let quick_deleted = store
        .delete_if_quick(quick.id)
        .expect("清理 QuickCard Bella 应成功执行");
    let remaining = store.list().expect("清理后应读取剩余 Definition");

    assert!(!explicit_deleted, "delete_if_quick 不得删除 Explicit Alan");
    assert!(
        quick_deleted,
        "delete_if_quick 应删除来源明确的 QuickCard Bella"
    );
    assert_eq!(
        remaining.len(),
        1,
        "初始 2 条仅清理 1 条 QuickCard 后应剩 1 条"
    );
    assert_eq!(
        remaining[0].id, explicit.id,
        "清理后保留的唯一 Definition 必须是 Explicit Alan"
    );
    assert_eq!(
        remaining[0].origin,
        AgentDefinitionOrigin::Explicit,
        "保留记录必须维持 Explicit 来源"
    );
    assert!(
        remaining.iter().all(|definition| definition.id != quick.id),
        "清理后不得残留 QuickCard Bella"
    );
}

/// AD-ORIGIN-3：关闭 QuickCard 时，即使残留 conversation tail 也必须删掉 Definition 并释放名称。
///
/// 数据构造（含关键数值的推导过程）：
///   quick definitions = 1 条：Bella
///   conversation tails = 1 条：definition_id = Bella.id，node_id = 999（与真实卡片 id 不一致，
///   模拟按 node_id 删除 tail 时永远删不到的历史残留）
///   清理后 definitions = 1 - 1 = 0；tails = 1 - 1 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 QuickCard Bella，并写入引用它的 conversation tail。
///   2. 调用 `delete_if_quick`：外键开启，必须在同一事务内先删 tail，再删 Definition。
///   3. 重新读取 Definition 与 tail，并再次创建 Bella 验证名称已释放。
///
/// 预期结果：
///   - 正断言：`delete_if_quick` 返回 Ok(true)，Definition 列表为空，tail 列表为空。
///   - 正断言：同名 Bella 可再次创建，说明内置名称被归还。
///   - 负断言：不得因 FOREIGN KEY 失败而保留孤儿 Definition（否则 30 个内置名被永久占用）。
#[test]
fn test_delete_if_quick_removes_definition_even_with_lingering_conversation_tail() {
    let temp = tempfile::tempdir().expect("应创建临时目录");
    let path = temp.path().join("project.sqlite");
    let store = AgentDefinitionStore::new(path.clone());
    let quick = store
        .create_quick(&NewAgentDefinition {
            name: "Bella".to_string(),
            runtime_profile: AgentProfile::Claude,
            workflow_description: String::new(),
        })
        .expect("应创建 QuickCard Bella");
    let tails = crate::agent_conversation_tail_store::AgentConversationTailStore::new(path);
    tails
        .upsert(
            &crate::agent_conversation_tail_store::StoredAgentConversationTail {
                definition_id: quick.id,
                node_id: 999,
                session_id: "s1".to_string(),
                agent_name: "Bella".to_string(),
                profile: "claude".to_string(),
                last_user_message: Some("hi".to_string()),
                last_agent_message: None,
                updated_at_unix: 1_700_000_000,
                availability:
                    crate::agent_conversation_tail_store::StoredConversationAvailability::Available,
                user_message_truncated: false,
                agent_message_truncated: false,
            },
        )
        .expect("应写入残留 tail");

    let deleted = store
        .delete_if_quick(quick.id)
        .expect("残留 tail 不得让 QuickCard 清理因外键失败");

    assert!(deleted, "QuickCard Definition 应被删除");
    assert!(
        store.list().expect("应读取 Definition").is_empty(),
        "不得残留孤儿 Definition"
    );
    assert!(
        tails.list().expect("应读取 tail").is_empty(),
        "引用该 Definition 的 tail 必须一并删除"
    );
    store
        .create(&NewAgentDefinition {
            name: "Bella".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: String::new(),
        })
        .expect("清理后内置名 Bella 必须可重新分配");
}

/// AD-ORIGIN-4：Settings 显式删除 Agent 时同样不得被残留 conversation tail 的外键阻塞。
///
/// 数据构造：Explicit Alan 1 条 + 引用它的 tail 1 条；删除后两者均为 0。
///
/// 执行过程：写入 tail → `delete(alan.id)` → 读取 Definition 与 tail。
///
/// 预期结果：
///   - 正断言：`delete` 返回 Ok(true)，Definition 与 tail 均为空。
///   - 负断言：不得返回外键错误。
#[test]
fn test_delete_removes_definition_with_conversation_tail() {
    let temp = tempfile::tempdir().expect("应创建临时目录");
    let path = temp.path().join("project.sqlite");
    let store = AgentDefinitionStore::new(path.clone());
    let alan = store
        .create(&NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: String::new(),
        })
        .expect("应创建 Alan");
    let tails = crate::agent_conversation_tail_store::AgentConversationTailStore::new(path);
    tails
        .upsert(
            &crate::agent_conversation_tail_store::StoredAgentConversationTail {
                definition_id: alan.id,
                node_id: 1,
                session_id: "s1".to_string(),
                agent_name: "Alan".to_string(),
                profile: "codex".to_string(),
                last_user_message: None,
                last_agent_message: None,
                updated_at_unix: 1_700_000_000,
                availability:
                    crate::agent_conversation_tail_store::StoredConversationAvailability::Available,
                user_message_truncated: false,
                agent_message_truncated: false,
            },
        )
        .expect("应写入 tail");

    let deleted = store.delete(alan.id).expect("残留 tail 不得阻塞显式删除");

    assert!(deleted, "Alan 应被删除");
    assert!(
        store.list().expect("应读取").is_empty(),
        "不得残留 Definition"
    );
    assert!(
        tails.list().expect("应读取 tail").is_empty(),
        "tail 必须一并删除"
    );
}

/// AD-ORIGIN-5：项目打开时清理已泄漏的 QuickCard Definition，只保留仍有画布卡片的记录。
///
/// 数据构造（含关键数值的推导过程）：
///   Explicit Alan 1 条；QuickCard Bella（有卡片，live）1 条；QuickCard Cora（无卡片，泄漏）1 条
///   初始总数 = 1 + 1 + 1 = 3；清理 1 条泄漏 → 剩余 3 - 1 = 2
///   Cora 另带 1 条 conversation tail（definition_id = Cora.id），验证外键不阻塞
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建三条 Definition 并给 Cora 写入 tail。
///   2. `purge_quick_except(&[Bella.id])` 一次性清理。
///   3. 重新 `list()`，并检查 tail。
///
/// 预期结果：
///   - 正断言：返回 1；剩余 Alan 与 Bella；Cora 的 tail 被删除。
///   - 负断言：Cora 不得残留；Explicit Alan 不得被清理（即使没有卡片）。
#[test]
fn test_purge_quick_except_removes_only_leaked_quick_definitions() {
    let temp = tempfile::tempdir().expect("应创建临时目录");
    let path = temp.path().join("project.sqlite");
    let store = AgentDefinitionStore::new(path.clone());
    let new = |name: &str| NewAgentDefinition {
        name: name.to_string(),
        runtime_profile: AgentProfile::Codex,
        workflow_description: String::new(),
    };
    let alan = store.create(&new("Alan")).expect("应创建 Alan");
    let bella = store.create_quick(&new("Bella")).expect("应创建 Bella");
    let cora = store.create_quick(&new("Cora")).expect("应创建 Cora");
    let tails = crate::agent_conversation_tail_store::AgentConversationTailStore::new(path);
    tails
        .upsert(
            &crate::agent_conversation_tail_store::StoredAgentConversationTail {
                definition_id: cora.id,
                node_id: 5,
                session_id: "s1".to_string(),
                agent_name: "Cora".to_string(),
                profile: "codex".to_string(),
                last_user_message: None,
                last_agent_message: None,
                updated_at_unix: 1_700_000_000,
                availability:
                    crate::agent_conversation_tail_store::StoredConversationAvailability::Available,
                user_message_truncated: false,
                agent_message_truncated: false,
            },
        )
        .expect("应写入 Cora 的 tail");

    let purged = store
        .purge_quick_except(&[bella.id])
        .expect("清理泄漏 Definition 不应失败");
    let remaining = store.list().expect("应读取剩余 Definition");

    assert_eq!(purged, 1, "3 条中仅 Cora 泄漏，应清理 1 条");
    assert_eq!(remaining.len(), 2, "初始 3 条清理 1 条后应剩 2 条");
    assert!(
        remaining.iter().any(|d| d.id == alan.id),
        "Explicit Alan 无卡片也必须保留"
    );
    assert!(
        remaining.iter().any(|d| d.id == bella.id),
        "有卡片的 QuickCard Bella 必须保留"
    );
    assert!(
        remaining.iter().all(|d| d.id != cora.id),
        "泄漏的 Cora 不得残留"
    );
    assert!(
        tails.list().expect("应读取 tail").is_empty(),
        "Cora 的 tail 必须一并删除"
    );
}
