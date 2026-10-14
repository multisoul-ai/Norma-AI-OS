mod support;
use norma::connector::*;
use std::sync::Arc;
/// 场景：令牌轮换及吊销后的新授权；临时凭据，随机Token。
/// 执行：生成、轮换、重载Auth、吊销、再生成；预期：宽限期共享epoch，吊销隔离，写限流独立。
#[test]
fn token_rotation_preserves_epoch_and_revoke_starts_new_namespace() {
    let store = Arc::new(support::Credentials::default());
    let auth = Auth::load(store.clone(), Arc::new(SystemClock)).expect("鉴权");
    let first = auth.generate().expect("随机令牌");
    let before = auth.authenticate(Some(&first)).expect("首身份");
    let second = auth.generate().expect("轮换");
    let rotated = auth.authenticate(Some(&second)).expect("轮换身份");
    assert_eq!(
        before.client_epoch(),
        rotated.client_epoch(),
        "宽限期同命名空间"
    );
    let reopened = Auth::load(store, Arc::new(SystemClock)).expect("重开");
    assert_eq!(
        reopened
            .authenticate(Some(&second))
            .expect("恢复身份")
            .client_epoch(),
        before.client_epoch(),
        "epoch落盘稳定"
    );
    for _ in 0..10 {
        auth.limit(&rotated, true).expect("十次写预算");
    }
    assert_eq!(
        auth.limit(&rotated, true).expect_err("写限流").0,
        "rate_limited",
        "第11次写拒绝"
    );
    auth.read(&rotated).expect("读预算独立");
    auth.revoke().expect("吊销");
    let next = auth.generate().expect("新授权");
    assert_ne!(
        auth.authenticate(Some(&next))
            .expect("新身份")
            .client_epoch(),
        before.client_epoch(),
        "新授权不能读取旧操作"
    );
    assert!(auth.authenticate(Some(&first)).is_err(), "旧Token失效");
}
