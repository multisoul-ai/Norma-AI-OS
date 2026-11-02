use super::*;
use std::{collections::HashMap, sync::Mutex};

#[derive(Default)]
struct Vault {
    values: Mutex<HashMap<String, String>>,
    deny: bool,
}
impl SecretVault for Vault {
    fn set(&self, account: &str, secret: &str) -> Result<(), JevConfigurationError> {
        if self.deny {
            return Err(JevConfigurationError::SecureStorage);
        }
        self.values
            .lock()
            .expect("隔离vault")
            .insert(account.into(), secret.into());
        Ok(())
    }
    fn get(&self, account: &str) -> Result<Option<String>, JevConfigurationError> {
        Ok(self.values.lock().expect("隔离vault").get(account).cloned())
    }
    fn delete(&self, account: &str) -> Result<(), JevConfigurationError> {
        self.values.lock().expect("隔离vault").remove(account);
        Ok(())
    }
}

/// 场景：Jev密钥必须经安全存储保存，重开配置可加载；拒绝写入时保留旧引用。
/// 构造：隔离SQLite与内存vault；用人工字符串fixture-key-one替代真人凭据。
/// 执行：保存并重新创建配置对象，读取凭据；随后模拟vault拒绝替换。
/// 预期：SQLite仅有非敏感引用，重新加载得到原密钥；失败不改变旧配置，生产始终禁用。
#[test]
fn voice_jev_secure_configuration_roundtrip_and_failed_replacement() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let store = storage::AppStore::initialize(&paths)
        .expect("隔离数据库")
        .settings_store();
    let vault = Vault::default();
    let settings = JevSettings::new(store.clone());
    settings.save_key("fixture-key-one", &vault).expect("保存");
    let reference = store
        .get_string(JEV_CREDENTIAL_REF)
        .expect("读取引用")
        .expect("引用");
    assert_ne!(reference, "fixture-key-one", "数据库必须只存引用，不存密钥");
    let reopened = JevSettings::new(store.clone());
    assert_eq!(
        reopened.load_key(&vault).expect("重开读取").as_deref(),
        Some("fixture-key-one"),
        "重开应从相同安全存储引用加载密钥"
    );
    let failed = reopened.save_key(
        "fixture-key-two",
        &Vault {
            deny: true,
            ..Vault::default()
        },
    );
    assert!(
        matches!(failed, Err(JevConfigurationError::SecureStorage)),
        "安全存储拒绝须明确失败"
    );
    assert_eq!(
        store.get_string(JEV_CREDENTIAL_REF).expect("旧引用"),
        Some(reference),
        "失败不能替换上一份已保存的配置"
    );
    assert!(
        !reopened.production_enabled(),
        "配置成功也不能启用Jev生产路由"
    );
}

/// 场景：清除凭据时SQLite引用删除失败，必须补偿恢复已删除的安全存储值。
/// 构造：隔离库已有fixture-key；vault在delete后临时把数据库路径替换成目录，模拟写失败。
/// 执行：调用clear、恢复数据库文件，再从新配置对象加载。
/// 预期：clear明确失败且原凭据仍可恢复；不能只在当前UI内存保留、重开后永久丢失。
#[test]
fn voice_jev_failed_clear_restores_secure_credential() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let store = storage::AppStore::initialize(&paths)
        .expect("隔离库")
        .settings_store();
    let settings = JevSettings::new(store.clone());
    let vault = Vault::default();
    settings.save_key("fixture-key", &vault).expect("保存");
    struct FailingDelete<'a> {
        vault: &'a Vault,
        database: std::path::PathBuf,
        backup: std::path::PathBuf,
    }
    impl SecretVault for FailingDelete<'_> {
        fn set(&self, a: &str, s: &str) -> Result<(), JevConfigurationError> {
            self.vault.set(a, s)
        }
        fn get(&self, a: &str) -> Result<Option<String>, JevConfigurationError> {
            self.vault.get(a)
        }
        fn delete(&self, a: &str) -> Result<(), JevConfigurationError> {
            self.vault.delete(a)?;
            std::fs::rename(&self.database, &self.backup).expect("注入数据库写错误");
            std::fs::create_dir(&self.database).expect("不可写数据库路径");
            Ok(())
        }
    }
    let failing = FailingDelete {
        vault: &vault,
        database: store.path().to_owned(),
        backup: temp.path().join("backup.sqlite"),
    };
    let result = settings.clear(&failing);
    std::fs::remove_dir(&failing.database).expect("恢复目录");
    std::fs::rename(&failing.backup, &failing.database).expect("恢复原数据库");
    assert!(
        matches!(result, Err(JevConfigurationError::SettingsStorage)),
        "引用删除失败必须返回存储错误"
    );
    assert!(
        JevSettings::new(store)
            .load_key(&vault)
            .expect("重开读取")
            .as_deref()
            == Some("fixture-key"),
        "清除失败后必须补偿恢复原Keychain值，不能留下悬空引用"
    );
}

/// 场景：替换凭据时旧Keychain条目暂时不能删除，必须记录可恢复的清理引用。
/// 构造：隔离库和vault已有fixture-old，替换时仅拒绝delete，不拒绝保存新凭据。
/// 执行：保存新Key并检查非敏感清理记录，再重开配置、恢复正常vault后加载。
/// 预期：新凭据可用；旧条目不被静默遗忘，重开时重试清理并移除已完成的记录。
#[test]
fn voice_jev_retired_credential_cleanup_is_recoverable() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let store = storage::AppStore::initialize(&paths)
        .expect("隔离库")
        .settings_store();
    let settings = JevSettings::new(store.clone());
    let vault = Vault::default();
    settings.save_key("fixture-old", &vault).expect("旧凭据");
    let old = store
        .get_string(JEV_CREDENTIAL_REF)
        .expect("读取")
        .expect("旧引用");
    struct DenyDelete<'a>(&'a Vault);
    impl SecretVault for DenyDelete<'_> {
        fn set(&self, a: &str, s: &str) -> Result<(), JevConfigurationError> {
            self.0.set(a, s)
        }
        fn get(&self, a: &str) -> Result<Option<String>, JevConfigurationError> {
            self.0.get(a)
        }
        fn delete(&self, _: &str) -> Result<(), JevConfigurationError> {
            Err(JevConfigurationError::SecureStorage)
        }
    }
    settings
        .save_key("fixture-new", &DenyDelete(&vault))
        .expect("新凭据应可保存");
    let pending = store
        .get_string("voice.jev.pending_cleanup.v1")
        .expect("读取清理记录");
    assert!(
        pending.as_deref().is_some_and(|v| v.contains(&old)),
        "清理失败必须留下非敏感引用供重试，不能丢失追踪"
    );
    let reopened = JevSettings::new(store.clone());
    assert!(
        reopened.load_key(&vault).expect("重开").as_deref() == Some("fixture-new"),
        "新配置应实际可重读"
    );
    assert!(
        vault.get(&old).expect("旧条目").is_none(),
        "恢复权限后必须清理旧条目"
    );
    assert!(
        store
            .get_string("voice.jev.pending_cleanup.v1")
            .expect("清理记录")
            .is_none(),
        "完成后不保留过期清理标记"
    );
}

/// 场景：本机Keychain与真实Jev服务均须实际工作，错误凭据须被服务拒绝。
/// 构造：隔离SQLite，从已授权环境读取测试凭据，只发送固定的人工合成Choice请求。
/// 执行：写入/读取Keychain、调用真实验证；再用人工无效Key验证失败，并清除本测试凭据。
/// 预期：实际返回固定模型版本与正耗时，错误Key为401或403；生产入口仍禁用。
#[test]
#[ignore = "Requires authorized TYPESAFE_API_KEY, macOS Keychain and network"]
fn voice_jev_real_configuration_keychain_and_connection() {
    let key = std::env::var("TYPESAFE_API_KEY").expect("需要已授权的本机环境凭据");
    let temp = tempfile::tempdir().expect("隔离目录");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let settings = JevSettings::new(
        storage::AppStore::initialize(&paths)
            .expect("隔离库")
            .settings_store(),
    );
    struct Cleanup(JevSettings);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = self.0.clear(&KeychainVault);
        }
    }
    let _cleanup = Cleanup(settings.clone());
    settings
        .save_key(&key, &KeychainVault)
        .expect("真实Keychain保存");
    let loaded = settings.load_key(&KeychainVault).expect("真实Keychain重读");
    assert!(
        loaded.as_deref() == Some(key.trim()),
        "真实安全存储重读必须一致，失败也不能打印密钥"
    );
    let result = verify_key(loaded.expect("已保存凭据")).expect("真实Jev合成验证");
    assert!(
        result.model.starts_with("jev-") && !result.model.ends_with("latest"),
        "必须收到实际模型版本，不能用请求别名伪造成功"
    );
    assert!(result.elapsed_ms > 0, "必须记录实际网络验证耗时");
    println!(
        "Jev verified model={} elapsed_ms={}",
        result.model, result.elapsed_ms
    );
    let rejected = verify_key("norma-synthetic-invalid-key".into());
    assert!(
        matches!(rejected, Err(JevConfigurationError::Http(401 | 403))),
        "错误凭据必须由真实服务明确拒绝，网络失败不能算凭据校验通过"
    );
    assert!(
        !settings.production_enabled(),
        "真实配置验证不得开启生产入口"
    );
}
