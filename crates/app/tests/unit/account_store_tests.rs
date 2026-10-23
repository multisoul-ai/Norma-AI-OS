use super::store::AppDbAccountStore;
use account::{
    AccountId, AccountSessionStore, Identity, Profile, Provider, Session, SESSION_SCHEMA_VERSION,
};

fn store(temp: &tempfile::TempDir) -> (AppDbAccountStore, storage::SettingsStore) {
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let app = storage::AppStore::initialize(&paths).expect("隔离测试库应能初始化");
    let settings = app.settings_store();
    (AppDbAccountStore::new(settings.clone()), settings)
}

fn session() -> Session {
    Session::new(
        "access".into(),
        "refresh".into(),
        4_000,
        Profile {
            account_id: AccountId::new("acc_1"),
            display_name: "Zhou Kai".into(),
            email: None,
            avatar_url: None,
            identities: vec![Identity {
                provider: Provider::GitHub,
                subject: "42".into(),
            }],
        },
    )
}

/// 会话保存后可读回，清除后回到未登录。
///
/// 数据构造：隔离测试库；会话 expires_at = 4_000。
/// 执行过程：save → load → clear → load。
/// 预期结果：
///   - 正断言：save 后 load 等于原会话
///   - 负断言：clear 后 load 为 None
#[test]
fn test_session_round_trips_and_clears() {
    let temp = tempfile::tempdir().expect("临时目录应可创建");
    let (store, _) = store(&temp);
    assert!(store.load().is_none(), "新库没有会话");
    store.save(&session()).expect("保存应成功");
    assert_eq!(store.load(), Some(session()), "保存后应能读回同一会话");
    store.clear().expect("清除应成功");
    assert!(store.load().is_none(), "清除后应为未登录");
}

/// 损坏或来自未来版本的会话按未登录处理，不崩溃。
///
/// 数据构造：直接向 settings 写入乱码，以及 schema_version = 当前 + 1 的 JSON。
/// 预期结果：负断言：两种情况 load 均为 None。
#[test]
fn test_corrupt_or_future_session_is_signed_out() {
    let temp = tempfile::tempdir().expect("临时目录应可创建");
    let (store, settings) = store(&temp);
    settings
        .set_string("account.session", "not json")
        .expect("写入应成功");
    assert!(store.load().is_none(), "乱码会话必须按未登录处理");
    let future = session().encode().replace(
        &format!("\"schema_version\":{SESSION_SCHEMA_VERSION}"),
        &format!("\"schema_version\":{}", SESSION_SCHEMA_VERSION + 1),
    );
    settings
        .set_string("account.session", &future)
        .expect("写入应成功");
    assert!(store.load().is_none(), "更高版本的会话必须按未登录处理");
}

/// 「稍后再说」标记独立于会话，且可复位。
///
/// 预期结果：正断言：默认 false，设置后 true，复位后 false；负断言：设置标记不产生会话。
#[test]
fn test_dismissed_marker_is_independent() {
    let temp = tempfile::tempdir().expect("临时目录应可创建");
    let (store, _) = store(&temp);
    assert!(!store.login_prompt_dismissed(), "默认未跳过");
    store.set_login_prompt_dismissed(true).expect("写入应成功");
    assert!(store.login_prompt_dismissed(), "写入后应为已跳过");
    assert!(store.load().is_none(), "跳过标记不应产生会话");
    store.set_login_prompt_dismissed(false).expect("复位应成功");
    assert!(!store.login_prompt_dismissed(), "复位后应为未跳过");
}
