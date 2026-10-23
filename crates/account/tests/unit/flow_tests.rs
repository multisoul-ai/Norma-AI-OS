use super::fixtures::{profile, session};
use super::*;

fn attempt(provider: Provider, seed: u8) -> Attempt {
    Attempt {
        provider,
        pkce: Pkce::from_entropy(&[seed; 32]),
        state: format!("state-{seed}"),
    }
}

fn awaiting(seed: u8) -> Flow {
    let mut flow = Flow::new();
    flow.handle(FlowEvent::Begin(attempt(Provider::GitHub, seed)));
    flow
}

fn exchanging(seed: u8) -> Flow {
    let mut flow = awaiting(seed);
    flow.handle(FlowEvent::CallbackReceived(format!(
        "/callback?code=c1&state=state-{seed}"
    )));
    flow
}

/// 开始登录：进入等待浏览器，并要求打开授权页。
///
/// 数据构造：GitHub，熵种子 1。
/// 预期结果：
///   - 正断言：状态 AwaitingBrowser；动作 OpenBrowser 携带 provider、challenge、state
///   - 负断言：动作中不含 verifier（verifier 只在交换时出现）
#[test]
fn test_begin_requests_browser() {
    let mut flow = Flow::new();
    let a = attempt(Provider::GitHub, 1);
    let action = flow.handle(FlowEvent::Begin(a.clone()));
    assert!(
        matches!(flow.state(), FlowState::AwaitingBrowser(_)),
        "应进入等待浏览器"
    );
    assert_eq!(
        action,
        Some(FlowAction::OpenBrowser(AuthorizeRequest {
            provider: Provider::GitHub,
            code_challenge: a.pkce.challenge().to_string(),
            state: "state-1".to_string(),
        })),
        "应请求以 challenge 与 state 打开授权页"
    );
    assert!(
        !format!("{action:?}").contains(a.pkce.verifier()),
        "开始动作不得包含 verifier"
    );
}

/// 合法回调进入交换阶段，并带上 code 与 verifier。
///
/// 预期结果：正断言：状态 Exchanging；动作 ExchangeCode 的 code = "c1"，verifier 为该次尝试的 verifier。
#[test]
fn test_valid_callback_triggers_exchange() {
    let mut flow = awaiting(2);
    let verifier = Pkce::from_entropy(&[2; 32]).verifier().to_string();
    let action = flow.handle(FlowEvent::CallbackReceived(
        "/callback?code=c1&state=state-2".to_string(),
    ));
    assert_eq!(
        flow.state(),
        &FlowState::Exchanging(Provider::GitHub),
        "应进入交换阶段"
    );
    assert_eq!(
        action,
        Some(FlowAction::ExchangeCode(TokenRequest {
            code: "c1".to_string(),
            code_verifier: verifier,
        })),
        "应携带 code 与本次尝试的 verifier"
    );
}

/// 交换成功：登录完成并要求持久化会话。
///
/// 预期结果：正断言：状态 SignedIn(profile)；动作 PersistSession(session)。
#[test]
fn test_exchange_success_signs_in_and_persists() {
    let mut flow = exchanging(3);
    let s = session(9_999);
    let action = flow.handle(FlowEvent::ExchangeSucceeded(s.clone()));
    assert_eq!(
        flow.state(),
        &FlowState::SignedIn(s.profile.clone()),
        "应进入已登录"
    );
    assert_eq!(
        action,
        Some(FlowAction::PersistSession(s)),
        "应要求持久化会话"
    );
}

/// state 不匹配与提供方拒绝都进入失败，且不发起交换。
///
/// 预期结果：
///   - 负断言：错误 state → Failed(StateMismatch)，动作 None
///   - 负断言：用户拒绝 → Failed(ProviderDenied("access_denied")),动作 None
#[test]
fn test_bad_callbacks_fail_without_exchange() {
    let mut flow = awaiting(4);
    let action = flow.handle(FlowEvent::CallbackReceived(
        "/callback?code=c1&state=evil".to_string(),
    ));
    assert_eq!(
        flow.state(),
        &FlowState::Failed {
            provider: Provider::GitHub,
            reason: FailReason::StateMismatch
        },
        "state 不匹配必须失败"
    );
    assert_eq!(action, None, "state 不匹配不得发起交换");

    let mut flow = awaiting(4);
    let action = flow.handle(FlowEvent::CallbackReceived(
        "/callback?error=access_denied&state=state-4".to_string(),
    ));
    assert_eq!(
        flow.state(),
        &FlowState::Failed {
            provider: Provider::GitHub,
            reason: FailReason::ProviderDenied("access_denied".to_string())
        },
        "用户拒绝必须失败并带原因"
    );
    assert_eq!(action, None, "拒绝授权不得发起交换");
}

/// 缺 code 的回调视为无效回调。
///
/// 预期结果：负断言：Failed(InvalidCallback)。
#[test]
fn test_callback_without_code_is_invalid() {
    let mut flow = awaiting(4);
    flow.handle(FlowEvent::CallbackReceived(
        "/callback?state=state-4".to_string(),
    ));
    assert_eq!(
        flow.state(),
        &FlowState::Failed {
            provider: Provider::GitHub,
            reason: FailReason::InvalidCallback
        },
        "缺少 code 应判为无效回调"
    );
}

/// 取消后，迟到的回调与交换结果都不得完成登录。
///
/// 数据构造：等待浏览器时取消；另一路在交换中取消后迟到 ExchangeSucceeded。
/// 预期结果：
///   - 正断言：取消后状态 Idle
///   - 负断言：迟到回调与迟到交换成功均被忽略，状态保持 Idle，动作 None
#[test]
fn test_cancel_ignores_late_events() {
    let mut flow = awaiting(5);
    flow.handle(FlowEvent::Cancel);
    assert_eq!(flow.state(), &FlowState::Idle, "取消应回到 Idle");
    let action = flow.handle(FlowEvent::CallbackReceived(
        "/callback?code=c1&state=state-5".to_string(),
    ));
    assert_eq!(
        (flow.state(), action),
        (&FlowState::Idle, None),
        "取消后的回调必须忽略"
    );

    let mut flow = exchanging(5);
    flow.handle(FlowEvent::Cancel);
    let action = flow.handle(FlowEvent::ExchangeSucceeded(session(1)));
    assert_eq!(
        (flow.state(), action),
        (&FlowState::Idle, None),
        "取消后迟到的交换成功不得登录，也不得持久化"
    );
}

/// 超时只对等待浏览器生效；交换中与空闲忽略超时。
///
/// 预期结果：正断言：等待中超时 → Failed(TimedOut)；负断言：交换中超时仍 Exchanging，Idle 超时仍 Idle。
#[test]
fn test_timeout_only_applies_while_waiting() {
    let mut flow = awaiting(6);
    flow.handle(FlowEvent::TimedOut);
    assert_eq!(
        flow.state(),
        &FlowState::Failed {
            provider: Provider::GitHub,
            reason: FailReason::TimedOut
        },
        "等待浏览器超时应失败"
    );
    let mut flow = exchanging(6);
    flow.handle(FlowEvent::TimedOut);
    assert_eq!(
        flow.state(),
        &FlowState::Exchanging(Provider::GitHub),
        "交换中不受浏览器超时影响"
    );
    let mut flow = Flow::new();
    flow.handle(FlowEvent::TimedOut);
    assert_eq!(flow.state(), &FlowState::Idle, "空闲时超时应忽略");
}

/// 重复回调与空闲回调被忽略。
///
/// 预期结果：负断言：交换中再收到回调状态不变且无动作；Idle 收到回调状态不变且无动作。
#[test]
fn test_duplicate_and_idle_callbacks_ignored() {
    let mut flow = exchanging(7);
    let action = flow.handle(FlowEvent::CallbackReceived(
        "/callback?code=c2&state=state-7".to_string(),
    ));
    assert_eq!(
        (flow.state(), action),
        (&FlowState::Exchanging(Provider::GitHub), None),
        "重复回调不得再次发起交换"
    );
    let mut flow = Flow::new();
    let action = flow.handle(FlowEvent::CallbackReceived(
        "/callback?code=c1&state=state-7".to_string(),
    ));
    assert_eq!(
        (flow.state(), action),
        (&FlowState::Idle, None),
        "未开始时的回调必须忽略"
    );
}

/// 交换失败进入 Failed，并可重试。
///
/// 预期结果：正断言：Failed(Exchange("boom"))；再次 Begin 回到 AwaitingBrowser。
#[test]
fn test_exchange_failure_then_retry() {
    let mut flow = exchanging(8);
    flow.handle(FlowEvent::ExchangeFailed("boom".to_string()));
    assert_eq!(
        flow.state(),
        &FlowState::Failed {
            provider: Provider::GitHub,
            reason: FailReason::Exchange("boom".to_string())
        },
        "交换失败应带原因"
    );
    flow.handle(FlowEvent::Begin(attempt(Provider::Google, 9)));
    assert!(
        matches!(flow.state(), FlowState::AwaitingBrowser(_)),
        "失败后允许重试"
    );
}

/// 重新开始会替换尝试：旧 state 的回调不再有效。
///
/// 预期结果：负断言：第二次 Begin 后，用第一次的 state 回调 → Failed(StateMismatch)。
#[test]
fn test_restart_invalidates_previous_attempt() {
    let mut flow = awaiting(1);
    flow.handle(FlowEvent::Begin(attempt(Provider::GitHub, 2)));
    let action = flow.handle(FlowEvent::CallbackReceived(
        "/callback?code=c1&state=state-1".to_string(),
    ));
    assert_eq!(action, None, "旧尝试的回调不得发起交换");
    assert_eq!(
        flow.state(),
        &FlowState::Failed {
            provider: Provider::GitHub,
            reason: FailReason::StateMismatch
        },
        "旧 state 应被判为不匹配"
    );
}

/// 已登录时不能再次开始登录；退出登录清会话并回到 Idle。
///
/// 预期结果：
///   - 负断言：SignedIn 下 Begin 被忽略，状态不变
///   - 正断言：SignOut → Idle，动作 ClearSession；Idle 下 SignOut 无动作
#[test]
fn test_signed_in_blocks_begin_and_sign_out_clears() {
    let p = profile(Provider::GitHub, "42");
    let mut flow = Flow::signed_in(p.clone());
    let action = flow.handle(FlowEvent::Begin(attempt(Provider::Google, 1)));
    assert_eq!(
        (flow.state(), action),
        (&FlowState::SignedIn(p), None),
        "已登录时不得重新开始"
    );
    let action = flow.handle(FlowEvent::SignOut);
    assert_eq!(flow.state(), &FlowState::Idle, "退出登录应回到 Idle");
    assert_eq!(
        action,
        Some(FlowAction::ClearSession),
        "退出登录应要求清除会话"
    );
    assert_eq!(
        flow.handle(FlowEvent::SignOut),
        None,
        "未登录时退出登录无动作"
    );
}
