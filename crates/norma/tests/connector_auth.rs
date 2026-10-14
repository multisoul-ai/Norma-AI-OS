use norma::connector::{Auth, Clock, MemoryKeychain};
use std::sync::{
    atomic::{AtomicI64, Ordering},
    Arc,
};
mod support;
use support::Credentials;
struct Time(AtomicI64);
impl Clock for Time {
    fn now(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}
/// 场景：生成、轮换、吊销；用可注入时钟推进到 24h 边界（24×60×60=86400）。
/// 预期：新旧令牌宽限内都成功，边界时旧令牌失效；吊销撤销全部凭据。
#[test]
fn rotation_and_revoke() {
    let clock = Arc::new(Time(AtomicI64::new(100)));
    let store = Arc::new(Credentials::default());
    let auth = Auth::load(
        store.clone(),
        Arc::new(MemoryKeychain::default()),
        clock.clone(),
    )
    .expect("鉴权");
    let old = auth.generate().expect("首次生成");
    assert!(auth.authenticate(Some(&old)).is_ok(), "有效令牌");
    let new = auth.generate().expect("轮换");
    clock.0.store(100 + 86399, Ordering::SeqCst);
    assert!(auth.authenticate(Some(&old)).is_ok(), "宽限最后一秒");
    clock.0.store(100 + 86400, Ordering::SeqCst);
    assert!(auth.authenticate(Some(&old)).is_err(), "宽限期满立即失效");
    assert!(auth.authenticate(Some(&new)).is_ok(), "新令牌继续有效");
    auth.revoke().expect("立即吊销");
    assert!(auth.authenticate(Some(&new)).is_err(), "所有令牌被吊销");
    assert!(
        store.hashes.lock().expect("哈希").is_empty(),
        "本地删除哈希"
    );
}

/// 场景：每令牌60次/分钟；用时钟推进1秒再到窗口边界，验证错误与恢复。
/// 预期：第61次拒绝、retry_after=59；60秒后读预算恢复。
#[test]
fn read_limit_and_lockout_boundaries() {
    let clock = Arc::new(Time(AtomicI64::new(100)));
    let auth = Auth::load(
        Arc::new(Credentials::default()),
        Arc::new(MemoryKeychain::default()),
        clock.clone(),
    )
    .expect("鉴权");
    let token = auth.generate().expect("随机令牌");
    let identity = auth.authenticate(Some(&token)).expect("有效");
    for _ in 0..60 {
        assert!(auth.read(&identity).is_ok(), "一分钟60次允许");
    }
    clock.0.store(101, Ordering::SeqCst);
    assert_eq!(
        auth.read(&identity),
        Err(("rate_limited", Some(59))),
        "第61次须给出剩余59秒"
    );
    clock.0.store(160, Ordering::SeqCst);
    assert!(auth.read(&identity).is_ok(), "窗口边界恢复");
    for _ in 0..9 {
        assert!(auth.authenticate(None).is_err(), "缺失鉴权失败");
    }
    assert!(
        auth.authenticate(Some(&token)).is_ok(),
        "成功允许且不会清除固定窗口的失败数"
    );
    for _ in 0..10 {
        assert!(auth.authenticate(Some("bad")).is_err(), "连续10次失败");
    }
    assert_eq!(auth.locked_until(), 1060, "160+15×60=1060封锁边界");
    assert!(
        auth.authenticate(Some(&token)).is_ok(),
        "封锁期间正确令牌始终允许"
    );
    clock.0.store(1059, Ordering::SeqCst);
    assert!(
        auth.authenticate(Some(&token)).is_ok(),
        "封锁最后一秒仍允许有效令牌"
    );
    assert!(auth.read(&identity).is_ok(), "失败洪水不能阻断已鉴权读取");
    clock.0.store(1060, Ordering::SeqCst);
    assert!(auth.authenticate(Some(&token)).is_ok(), "15分钟后解锁");
}

/// 场景：随机生成令牌，经Keychain保存并用同一哈希存储重启鉴权，随后吊销。
/// 预期：格式符合32字节base64url，Keychain可读明文、本地仅摘要；重启不显示令牌，吊销删除Keychain。
#[test]
fn keychain_plaintext_hash_only_store_and_restart() {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use norma::connector::Keychain;
    use sha2::{Digest, Sha256};
    let store = Arc::new(Credentials::default());
    let keychain = Arc::new(MemoryKeychain::default());
    let clock = Arc::new(Time(AtomicI64::new(100)));
    let auth = Auth::load(store.clone(), keychain.clone(), clock.clone()).expect("鉴权");
    let token = auth.generate().expect("生成");
    let encoded = token.strip_prefix("nrm_live_").expect("规范前缀");
    assert_eq!(
        URL_SAFE_NO_PAD.decode(encoded).expect("base64url").len(),
        32,
        "CSPRNG令牌32字节"
    );
    assert!(
        URL_SAFE_NO_PAD.decode(encoded).expect("随机字节").len() * 8 >= 128,
        "32字节×8=256位，达到128位熵要求；生成器使用ring CSPRNG"
    );
    let digest: [u8; 32] = Sha256::digest(token.as_bytes()).into();
    let account = format!("token-{}", hex::encode(digest));
    assert!(
        keychain
            .get(&account)
            .expect("钥匙串")
            .is_some_and(|v| v == token.as_bytes()),
        "Keychain保存明文，断言不输出秘密"
    );
    assert_eq!(
        store.hashes.lock().expect("存储")[0].hash,
        digest,
        "本地只存哈希"
    );
    drop(auth);
    let restarted = Auth::load(store, keychain.clone(), clock).expect("重启");
    assert!(
        restarted.authenticate(Some(&token)).is_ok(),
        "重启恢复哈希授权"
    );
    restarted.revoke().expect("吊销");
    assert!(
        keychain.get(&account).expect("钥匙串").is_none(),
        "吊销删除钥匙串明文"
    );
}

/// 场景：两个来源各自累计失败，固定窗口15×60=900秒；正确令牌穿过锁定。
/// 构造：A在100秒失败9次、B失败1次；成功后A再失败1次，推进到999/1000秒。
/// 预期：成功不重置失败窗口，A独立锁定900秒；B未锁定，旧窗口和锁定均可恢复。
#[test]
fn source_ip_windows_are_independent_and_valid_tokens_bypass_lockout() {
    let clock = Arc::new(Time(AtomicI64::new(100)));
    let auth = Auth::load(
        Arc::new(Credentials::default()),
        Arc::new(MemoryKeychain::default()),
        clock.clone(),
    )
    .expect("鉴权");
    let token = auth.generate().expect("令牌");
    let a = "198.51.100.1".parse().expect("A IP");
    let b = "198.51.100.2".parse().expect("B IP");
    for _ in 0..9 {
        assert!(auth.authenticate_from(a, Some("bad")).is_err(), "A失败");
    }
    assert!(auth.authenticate_from(b, None).is_err(), "B失败");
    assert_eq!(auth.locked_until_for(a), 0, "9次未达到10次阈值");
    let identity = auth.authenticate_from(a, Some(&token)).expect("A正确令牌");
    assert!(auth.authenticate_from(a, None).is_err(), "A第10次失败");
    assert_eq!(auth.locked_until_for(a), 1000, "100+900=1000，仅锁定A");
    assert_eq!(auth.locked_until_for(b), 0, "B独立预算");
    for _ in 0..100 {
        assert!(auth.authenticate_from(a, None).is_err(), "锁定洪水拒绝");
    }
    assert!(
        auth.authenticate_from(a, Some(&token)).is_ok(),
        "锁定A仍可使用正确令牌"
    );
    assert!(
        auth.authenticate_from(b, Some(&token)).is_ok(),
        "其他IP洪水不影响B"
    );
    assert!(auth.read(&identity).is_ok(), "已授权读取继续工作");
    clock.0.store(999, Ordering::SeqCst);
    assert_eq!(auth.locked_until_for(a), 1000, "锁定最后一秒");
    clock.0.store(1000, Ordering::SeqCst);
    assert!(
        auth.authenticate_from(a, None).is_err(),
        "边界后开始新失败窗口"
    );
    assert_eq!(auth.locked_until_for(a), 0, "锁定恢复");
    for _ in 0..8 {
        assert!(
            auth.authenticate_from(b, None).is_err(),
            "B旧窗口失效后仅8次"
        );
    }
    assert_eq!(auth.locked_until_for(b), 0, "B在新窗口未达阈值");
}
