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
