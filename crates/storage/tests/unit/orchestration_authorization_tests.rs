use super::super::{OrchestrationAuthorizationCache, OrchestrationAuthorizationStore};
use norma::orchestration::OrchestrationProvider;

/// Provider 长期授权：SQLite 只保存 Token 哈希，同 Token 仅可用于原 Provider。
///
/// 数据构造（含关键数值的推导过程）：
///   Codex token = 64 个十六进制字符 "aa…"
///   Claude token = 64 个十六进制字符 "bb…"
///   Provider 数 = 2；共享 Token 尝试 = 1 次（Codex token → Claude）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 为 Codex 与 Claude 分别登记长期 Token。
///   2. 读取审计记录并用公开授权校验接口验证原 Provider。
///   3. 把 Codex Token 用于 Claude，验证跨 Provider 被拒绝。
///
/// 预期结果：
///   - 正断言：两个原 Provider Token 均授权成功，数据库存在两条独立记录。
///   - 正断言：记录只含固定长度哈希，不等于原始 Token。
///   - 负断言：Codex Token 不得通过 Claude 授权，也不得出现在审计记录 Debug 文本。
#[test]
fn provider_tokens_are_hashed_and_cannot_cross_provider_boundaries() {
    let temp = tempfile::tempdir().expect("授权测试应创建临时目录");
    let store = OrchestrationAuthorizationStore::new(temp.path().join("app.sqlite"));
    let codex_token = "aa".repeat(32);
    let claude_token = "bb".repeat(32);

    store
        .authorize(OrchestrationProvider::Codex, &codex_token, "1.0.0")
        .expect("Codex Token 应成功登记");
    store
        .authorize(OrchestrationProvider::Claude, &claude_token, "1.0.0")
        .expect("Claude Token 应成功登记");
    let records = store.list().expect("授权审计记录应可读取");

    assert_eq!(
        records.len(),
        2,
        "两个 Provider 必须各自拥有一条独立授权记录"
    );
    assert!(
        store
            .is_authorized(OrchestrationProvider::Codex, &codex_token)
            .expect("Codex 授权校验应成功"),
        "原始 Codex Token 必须通过 Codex 授权校验"
    );
    assert!(
        store
            .is_authorized(OrchestrationProvider::Claude, &claude_token)
            .expect("Claude 授权校验应成功"),
        "原始 Claude Token 必须通过 Claude 授权校验"
    );
    assert!(
        !store
            .is_authorized(OrchestrationProvider::Claude, &codex_token)
            .expect("跨 Provider 授权校验应返回明确 false"),
        "Codex Token 不得跨 Provider 用于 Claude"
    );
    assert_eq!(
        records[0].token_hash.len(),
        64,
        "SHA-256 Token 哈希必须编码为固定 64 个十六进制字符"
    );
    assert_ne!(
        records[0].token_hash, codex_token,
        "SQLite 授权记录不得保存 Codex Token 明文"
    );
    assert!(
        !format!("{records:?}").contains(&codex_token),
        "授权审计记录 Debug 文本不得泄露 Codex Token 明文"
    );
    assert!(
        !format!("{records:?}").contains(&claude_token),
        "授权审计记录 Debug 文本不得泄露 Claude Token 明文"
    );
}

/// Provider 授权撤销：卸载后立即拒绝旧 Token，同时保留撤销审计时间。
///
/// 数据构造（含关键数值的推导过程）：
///   Provider = OpenCode
///   token = "cc"×32
///   状态转换 = enabled(true) → revoked(false)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 登记 OpenCode Token 并验证当前可用。
///   2. 调用 revoke(OpenCode)。
///   3. 再次校验旧 Token并读取同一 Provider 审计记录。
///
/// 预期结果：
///   - 正断言：撤销前可用，撤销后 enabled=false 且 revoked_at 存在。
///   - 负断言：撤销后旧 Token 不得继续通过授权，记录不得被物理删除。
#[test]
fn revocation_invalidates_token_immediately_but_preserves_audit_record() {
    let temp = tempfile::tempdir().expect("撤销测试应创建临时目录");
    let store = OrchestrationAuthorizationStore::new(temp.path().join("app.sqlite"));
    let token = "cc".repeat(32);
    store
        .authorize(OrchestrationProvider::OpenCode, &token, "1.0.0")
        .expect("OpenCode Token 应成功登记");
    let authorized_before = store
        .is_authorized(OrchestrationProvider::OpenCode, &token)
        .expect("撤销前授权校验应成功");

    store
        .revoke(OrchestrationProvider::OpenCode)
        .expect("OpenCode 授权撤销应成功");
    let authorized_after = store
        .is_authorized(OrchestrationProvider::OpenCode, &token)
        .expect("撤销后授权校验应返回明确 false");
    let records = store.list().expect("撤销后审计记录仍应可读取");

    assert!(
        authorized_before,
        "撤销前原始 OpenCode Token 必须处于有效状态"
    );
    assert!(
        !authorized_after,
        "卸载撤销后旧 OpenCode Token 必须立即失效"
    );
    assert_eq!(
        records.len(),
        1,
        "撤销授权必须保留一条 OpenCode 审计记录而非物理删除"
    );
    assert!(
        !records[0].enabled,
        "撤销后的 OpenCode 授权记录必须 enabled=false"
    );
    assert!(
        records[0].revoked_at_unix.is_some(),
        "撤销后的 OpenCode 授权记录必须保存 revoked_at_unix"
    );
    assert_ne!(records.len(), 0, "授权撤销不得删除用于排障和审计的历史记录");
}

/// 前台授权快照：后台加载哈希后，Socket 热路径可在内存中校验且保持 Provider 隔离。
///
/// 数据构造（含关键数值的推导过程）：
///   Codex token = "dd"×32
///   Claude token = "ee"×32
///   SQLite 后台加载记录数 = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 Store 登记两个 Provider Token 并读取只含哈希的记录。
///   2. 用记录构造 OrchestrationAuthorizationCache。
///   3. 在不再访问 SQLite 的情况下校验原 Token 与跨 Provider Token。
///
/// 预期结果：
///   - 正断言：Codex、Claude 原 Token 均通过内存快照。
///   - 负断言：Codex Token 不得通过 Claude，缓存 Debug 不得泄露明文。
#[test]
fn authorization_cache_verifies_hashed_records_without_sqlite_on_hot_path() {
    let temp = tempfile::tempdir().expect("授权缓存测试应创建临时目录");
    let store = OrchestrationAuthorizationStore::new(temp.path().join("app.sqlite"));
    let codex_token = "dd".repeat(32);
    let claude_token = "ee".repeat(32);
    store
        .authorize(OrchestrationProvider::Codex, &codex_token, "1.0.0")
        .expect("Codex Token 应成功登记");
    store
        .authorize(OrchestrationProvider::Claude, &claude_token, "1.0.0")
        .expect("Claude Token 应成功登记");
    let cache =
        OrchestrationAuthorizationCache::new(store.list().expect("后台应能加载授权哈希记录"));

    assert!(
        cache.is_authorized(OrchestrationProvider::Codex, &codex_token),
        "内存授权快照必须接受原始 Codex Token"
    );
    assert!(
        cache.is_authorized(OrchestrationProvider::Claude, &claude_token),
        "内存授权快照必须接受原始 Claude Token"
    );
    assert!(
        !cache.is_authorized(OrchestrationProvider::Claude, &codex_token),
        "内存授权快照不得接受跨 Provider Token"
    );
    assert!(
        !format!("{cache:?}").contains(&codex_token),
        "授权缓存 Debug 不得泄露 Codex Token 明文"
    );
}
