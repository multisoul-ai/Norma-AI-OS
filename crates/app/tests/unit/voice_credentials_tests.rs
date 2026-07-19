use super::super::provider_values::{
    AgentProviderKind, SavedProviderProfile, KEY_AGENT_OPENAI_API_KEY,
    KEY_AGENT_OPENAI_CREDENTIAL_REF,
};
use super::super::values::KEY_OPENAI_API_KEY;
use super::super::{VoiceSettingsField, VoiceSettingsState};
use crate::settings::voice::{VoiceRequirement, VoiceRequirementState};

/// 新保存的 OpenAI-compatible API Key：明文只进入凭据存储，SQLite 仅保存非敏感引用
///
/// 数据构造（含关键数值的推导过程）：
///   API Key 明文长度 = 24 chars（只用于测试内存，不写日志）
///   后台操作数 = 4：Keychain Set + credential_ref Set + canonical plaintext Delete + legacy plaintext Delete
///   SQLite 明文键数量 = 2（canonical + legacy），期望保留数量 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用临时 SettingsStore 加载全新 Voice state，默认 Provider 为 OpenAI-compatible
///   2. 提交 API Key，确认前台路径只生成待执行操作
///   3. 在测试线程执行待写操作，模拟后台持久化
///   4. 从 SQLite 读取引用与两个历史明文键，再从注入的内存凭据存储读取引用
///
/// 预期结果：
///   - 正断言：SQLite 有 credential_ref，凭据存储可通过该引用取回密钥
///   - 正断言：重载 state 仍显示 credential configured
///   - 负断言：canonical 和 legacy SQLite 键均不得包含或保留 API Key 明文
#[test]
fn api_key_persistence_uses_credential_store_and_keeps_sqlite_secret_free() {
    let temp = tempfile::tempdir().expect("credential test tempdir should create");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store.clone());
    assert_eq!(
        state.values().agent_provider(),
        AgentProviderKind::OpenAiCompatible,
        "fresh credentials test must use the product-default OpenAI-compatible provider"
    );

    let accepted = state.commit_edit_value(
        VoiceSettingsField::ApiKey,
        "sk-keychain-test-secret".to_string(),
    );
    let writes = state.take_pending_writes();

    assert!(
        accepted.is_some(),
        "non-empty API Key must be accepted for background credential persistence"
    );
    assert_eq!(
        writes.len(),
        4,
        "OpenAI API Key save must queue Keychain Set, ref Set, and two plaintext deletes"
    );
    assert!(
        store
            .get_string(KEY_AGENT_OPENAI_CREDENTIAL_REF)
            .expect("pre-write credential ref query should succeed")
            .is_none(),
        "foreground save path must not synchronously write the credential ref"
    );
    for write in writes {
        write
            .execute()
            .expect("simulated background credential write should succeed");
    }
    state.record_persistence_result(Ok(()));

    let credential_ref = store
        .get_string(KEY_AGENT_OPENAI_CREDENTIAL_REF)
        .expect("credential ref query should succeed")
        .expect("SQLite must persist a non-sensitive Keychain account reference");
    let keychain_secret = VoiceSettingsState::load_credential(&credential_ref)
        .expect("test credential store lookup should succeed");
    assert_eq!(
        keychain_secret.as_deref(),
        Some("sk-keychain-test-secret"),
        "credential reference must resolve to the saved secret in the injected test store"
    );
    assert!(
        store
            .get_string(KEY_AGENT_OPENAI_API_KEY)
            .expect("canonical plaintext key query should succeed")
            .is_none(),
        "canonical SQLite API Key field must be deleted after Keychain persistence"
    );
    assert!(
        store
            .get_string(KEY_OPENAI_API_KEY)
            .expect("legacy plaintext key query should succeed")
            .is_none(),
        "legacy SQLite API Key field must be deleted after Keychain persistence"
    );
    let reloaded = VoiceSettingsState::load_from_store(store);
    assert!(
        reloaded
            .values()
            .active_provider_profile()
            .credential_configured(),
        "credential_ref must preserve the configured UI state across reload"
    );
    assert!(
        reloaded
            .values()
            .active_provider_profile()
            .api_key()
            .is_none(),
        "synchronous reload must not copy Keychain secret material into SQLite-backed values"
    );
}

/// 旧版 SQLite API Key 迁移：写入 Keychain 前继续使用内存中的旧密钥，不发起提前读取
///
/// 数据构造（含关键数值的推导过程）：
///   legacy API Key 数量 = 1
///   自动迁移后台操作数 = 4：Keychain Set + credential_ref Set + canonical Delete + legacy Delete
///   迁移前 Keychain 读取引用数 = 0（内存已有可用密钥）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在临时 SQLite 写入 OpenAI-compatible 的 legacy API Key
///   2. 加载 Voice state，自动生成 credential_ref 与四个迁移操作
///   3. 在迁移操作尚未执行时查询需要从 Keychain 加载的引用
///
/// 预期结果：
///   - 正断言：旧密钥在迁移期间仍留在内存，Provider 可继续验证
///   - 正断言：迁移仍完整排队，后续可原子地先写凭据再删明文
///   - 负断言：Keychain 写入完成前不得按新引用读取并用空值覆盖内存密钥
#[test]
fn legacy_api_key_migration_does_not_race_keychain_loading() {
    let temp = tempfile::tempdir().expect("legacy credential test tempdir should create");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    store
        .set_string(KEY_OPENAI_API_KEY, "sk-legacy-migration-secret")
        .expect("test setup must persist the legacy API Key");

    let mut state = VoiceSettingsState::load_from_store(store);
    let refs = state.credential_refs();
    let writes = state.take_pending_writes();

    assert_eq!(
        state
            .values()
            .active_provider_profile()
            .api_key()
            .as_deref(),
        Some("sk-legacy-migration-secret"),
        "legacy API Key must remain available in memory until Keychain migration finishes"
    );
    assert_eq!(
        writes.len(),
        4,
        "legacy OpenAI API Key must queue the complete four-operation migration"
    );
    assert!(
        refs.is_empty(),
        "credential refresh must not read a not-yet-written Keychain reference and overwrite the in-memory legacy secret"
    );
    assert_ne!(
        state
            .values()
            .active_provider_profile()
            .api_key()
            .as_deref(),
        None,
        "migration must not clear the only usable provider credential before background persistence"
    );
}

/// Keychain 引用与遗留明文并存：必须先读 Keychain，并在成功后继续删除 SQLite 明文
///
/// 数据构造（含关键数值的推导过程）：
///   credential_ref = 1
///   Keychain secret = sk-keychain-authoritative
///   SQLite plaintext = 2 个键（canonical + legacy），内容均为旧值
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 预先写入测试凭据存储、credential_ref 与两个旧明文键
///   2. 重载 state，确认同步阶段没有选择 SQLite 明文
///   3. 应用与当前 account 匹配的 Keychain 读取结果并执行清理操作
///
/// 预期结果：
///   - 正断言：内存最终使用 Keychain secret
///   - 负断言：两个 SQLite 明文键均被删除，旧明文不再拥有优先级
#[test]
fn keychain_reference_wins_over_coexisting_plaintext_and_cleans_it_up() {
    let temp = tempfile::tempdir().expect("Keychain priority tempdir should create");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let account = "openai-existing-account";
    super::super::PendingVoiceSettingWrite::StoreCredential {
        account: account.to_string(),
        secret: "sk-keychain-authoritative".to_string(),
    }
    .execute()
    .expect("test setup must store the authoritative Keychain secret");
    store
        .set_string(KEY_AGENT_OPENAI_CREDENTIAL_REF, account)
        .expect("test setup must persist credential reference");
    store
        .set_string(KEY_AGENT_OPENAI_API_KEY, "sk-stale-canonical")
        .expect("test setup must persist stale canonical plaintext");
    store
        .set_string(KEY_OPENAI_API_KEY, "sk-stale-legacy")
        .expect("test setup must persist stale legacy plaintext");

    let mut state = VoiceSettingsState::load_from_store(store.clone());
    assert!(
        state.values().active_provider_profile().api_key().is_none(),
        "credential_ref must prevent SQLite plaintext from becoming the active secret"
    );
    let applied = state.apply_loaded_credential(
        AgentProviderKind::OpenAiCompatible,
        account,
        Ok(Some("sk-keychain-authoritative".to_string())),
    );
    let writes = state.take_pending_writes();
    for write in writes {
        write
            .execute()
            .expect("post-Keychain plaintext cleanup should succeed");
    }

    assert!(applied, "matching Keychain result must be applied");
    assert_eq!(
        state.values().active_provider_profile().api_key(),
        Some("sk-keychain-authoritative"),
        "Keychain secret must be the only active credential"
    );
    assert!(
        store
            .get_string(KEY_AGENT_OPENAI_API_KEY)
            .expect("canonical plaintext query should succeed")
            .is_none(),
        "canonical SQLite plaintext must be deleted after Keychain succeeds"
    );
    assert!(
        store
            .get_string(KEY_OPENAI_API_KEY)
            .expect("legacy plaintext query should succeed")
            .is_none(),
        "legacy SQLite plaintext must be deleted after Keychain succeeds"
    );
}

/// 过期 Keychain 读取：用户 Clear 后到达的旧异步结果不得重新注入 secret
///
/// 数据构造（含关键数值的推导过程）：
///   初始 credential_ref = old-account
///   用户操作 = Clear 1 次
///   旧异步结果 = secret 1 个
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从带 old-account 引用的 state 开始异步读取语义
///   2. 在结果返回前执行 Clear，使当前引用变为 None
///   3. 尝试应用 old-account 的旧结果
///
/// 预期结果：
///   - 正断言：旧结果被明确拒绝
///   - 负断言：secret 不得回到内存，Configured 状态不得复活
#[test]
fn stale_keychain_load_is_rejected_after_user_clears_credential() {
    let saved = super::super::SavedVoiceSettings {
        agent_provider: Some(AgentProviderKind::OpenAiCompatible),
        openai_profile: SavedProviderProfile {
            credential_ref: Some("old-account".to_string()),
            ..SavedProviderProfile::default()
        },
        ..super::super::SavedVoiceSettings::default()
    };
    let mut state =
        VoiceSettingsState::from_sources(saved, super::super::EnvVoiceSettings::default());

    state.clear_agent_api_key();
    let applied = state.apply_loaded_credential(
        AgentProviderKind::OpenAiCompatible,
        "old-account",
        Ok(Some("sk-stale-result".to_string())),
    );

    assert!(
        !applied,
        "Keychain result for a cleared account must be rejected"
    );
    assert!(
        state.values().active_provider_profile().api_key().is_none(),
        "stale Keychain result must not restore the cleared secret"
    );
    assert!(
        !state
            .values()
            .active_provider_profile()
            .credential_configured(),
        "stale Keychain result must not restore the Configured UI state"
    );
}

/// Keychain 引用存在但条目缺失：Configured 状态必须撤销并清理失效引用
///
/// 数据构造（含关键数值的推导过程）：
///   credential_ref = missing-account
///   Keychain lookup = Ok(None)
///   legacy plaintext fallback = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造只有引用、没有内存 secret 的 OpenAI-compatible profile
///   2. 应用 Keychain “未找到条目”的结果
///   3. 检查 profile 与 Provider 硬门槛
///
/// 预期结果：
///   - 正断言：失效引用被撤销，Provider 进入 Failed 以提示修复
///   - 负断言：UI 不得继续显示 Configured，也不得返回可验证 secret
#[test]
fn missing_keychain_item_clears_configured_state_and_fails_provider_gate() {
    let saved = super::super::SavedVoiceSettings {
        agent_provider: Some(AgentProviderKind::OpenAiCompatible),
        openai_profile: SavedProviderProfile {
            credential_ref: Some("missing-account".to_string()),
            ..SavedProviderProfile::default()
        },
        ..super::super::SavedVoiceSettings::default()
    };
    let mut state =
        VoiceSettingsState::from_sources(saved, super::super::EnvVoiceSettings::default());

    let applied = state.apply_loaded_credential(
        AgentProviderKind::OpenAiCompatible,
        "missing-account",
        Ok(None),
    );

    assert!(
        !applied,
        "missing Keychain item must not produce a usable credential"
    );
    assert_eq!(
        state.requirement_state(VoiceRequirement::AgentProvider),
        VoiceRequirementState::Failed,
        "missing Keychain item must fail the Provider readiness gate"
    );
    assert!(
        !state
            .values()
            .active_provider_profile()
            .credential_configured(),
        "missing Keychain item must clear the Configured UI state"
    );
    assert!(
        state.values().active_provider_profile().api_key().is_none(),
        "missing Keychain item must not leave secret material in memory"
    );
}

/// Keychain 写入失败：只存在于内存的新密钥不得让 Provider 看起来可持久使用
///
/// 数据构造（含关键数值的推导过程）：
///   新 API Key = 1
///   无 SettingsStore，因此后台批次 = Keychain store(1)
///   模拟后台持久化结果 = Err 1 次
///   先前已保存 credential_ref = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. fresh state 提交新 API Key，进入 credential pending
///   2. 取走包含一项 Keychain store 的后台批次。
///   3. 模拟该批次写入失败并触发 rollback。
///   4. 检查内存 profile 与 Provider readiness。
///
/// 预期结果：
///   - 正断言：Provider 门槛为 Failed，提示用户修复保存问题
///   - 负断言：失败密钥和 Configured 状态均不得留在内存
#[test]
fn failed_keychain_write_rolls_back_unpersisted_credential() {
    let mut state = VoiceSettingsState::from_sources(
        super::super::SavedVoiceSettings::default(),
        super::super::EnvVoiceSettings::default(),
    );
    state.commit_edit_value(VoiceSettingsField::ApiKey, "sk-will-fail".to_string());
    assert!(
        state.credential_persistence_pending(),
        "new API Key must remain pending until background persistence finishes"
    );
    let writes = state.take_pending_writes();
    assert_eq!(
        writes.len(),
        1,
        "session-only state must produce exactly one Keychain store write"
    );

    state.record_persistence_result(Err("simulated Keychain denial".to_string()));

    assert_eq!(
        state.requirement_state(VoiceRequirement::AgentProvider),
        VoiceRequirementState::Failed,
        "Keychain persistence failure must fail the Provider readiness gate"
    );
    assert!(
        state.values().active_provider_profile().api_key().is_none(),
        "unpersisted API Key must be removed from memory after rollback"
    );
    assert!(
        !state
            .values()
            .active_provider_profile()
            .credential_configured(),
        "Keychain failure must not leave the UI claiming the credential is configured"
    );
}
