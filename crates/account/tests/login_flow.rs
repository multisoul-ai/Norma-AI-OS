//! 只用公开 API 与内存假实现，跑通完整登录、失败、取消、退出。

mod support;

use account::{AccountSessionStore, AuthError, FailReason, Flow, FlowEvent, FlowState, Provider};
use support::{attempt, drive, FakeApi, MemStore};

/// 完整登录：开始 → 回调 → 交换 → 持久化；退出后清除。
///
/// 数据构造：Google，state = "st"，熵全 9；假 API 返回过期时间 10_000 的会话。
/// 执行过程：Begin → 回调 /callback?code=c&state=st → 自动交换并持久化 → SignOut。
/// 预期结果：
///   - 正断言：交换请求带 code "c" 与 PKCE verifier；存储里有会话；SignOut 后存储为空、状态 Idle
///   - 负断言：未调用任何真实网络或 AppDB（全程内存假实现）
#[test]
fn test_full_login_and_sign_out_with_fakes() {
    let (api, store) = (FakeApi::ok(), MemStore::default());
    let mut flow = Flow::new();
    drive(
        &mut flow,
        &api,
        &store,
        FlowEvent::Begin(attempt(Provider::Google)),
    );
    drive(
        &mut flow,
        &api,
        &store,
        FlowEvent::CallbackReceived("/callback?code=c&state=st".into()),
    );

    assert!(
        matches!(flow.state(), FlowState::SignedIn(_)),
        "登录后应为 SignedIn"
    );
    let seen = api.seen.borrow();
    assert_eq!(seen.len(), 1, "应恰好交换一次");
    assert_eq!(seen[0].code, "c", "交换请求应带回调 code");
    assert_eq!(
        seen[0].code_verifier.len(),
        43,
        "交换请求应带 43 字符 verifier"
    );
    assert!(store.load().is_some(), "会话应已持久化");

    drive(&mut flow, &api, &store, FlowEvent::SignOut);
    assert_eq!(flow.state(), &FlowState::Idle, "退出后应为 Idle");
    assert!(store.load().is_none(), "退出后存储应清空");
}

/// 交换失败：不持久化，可重试。
///
/// 预期结果：正断言：Failed(Exchange)；负断言：存储仍为空。
#[test]
fn test_exchange_failure_does_not_persist() {
    let (api, store) = (
        FakeApi::failing(AuthError::Network("down".into())),
        MemStore::default(),
    );
    let mut flow = Flow::new();
    drive(
        &mut flow,
        &api,
        &store,
        FlowEvent::Begin(attempt(Provider::GitHub)),
    );
    drive(
        &mut flow,
        &api,
        &store,
        FlowEvent::CallbackReceived("/callback?code=c&state=st".into()),
    );
    assert!(
        matches!(
            flow.state(),
            FlowState::Failed {
                reason: FailReason::Exchange(_),
                ..
            }
        ),
        "交换失败应进入 Failed(Exchange)"
    );
    assert!(store.load().is_none(), "失败时不得持久化会话");
}

/// 取消与跳过标记：取消不产生会话；跳过标记独立于会话。
///
/// 预期结果：负断言：取消后无会话、API 未被调用；正断言：dismissed 标记可读写且不影响会话。
#[test]
fn test_cancel_and_dismiss_marker() {
    let (api, store) = (FakeApi::ok(), MemStore::default());
    let mut flow = Flow::new();
    drive(
        &mut flow,
        &api,
        &store,
        FlowEvent::Begin(attempt(Provider::GitHub)),
    );
    drive(&mut flow, &api, &store, FlowEvent::Cancel);
    assert!(api.seen.borrow().is_empty(), "取消后不得调用交换");
    assert!(store.load().is_none(), "取消后不得有会话");

    assert!(!store.login_prompt_dismissed(), "默认未跳过");
    store
        .set_login_prompt_dismissed(true)
        .expect("内存存储不应失败");
    assert!(store.login_prompt_dismissed(), "跳过标记应可写入");
    assert!(store.load().is_none(), "跳过标记不应产生会话");
}
