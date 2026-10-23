//! 登录状态机：事件进、状态与待执行动作出，不做 IO。
//!
//! 取消、超时、重复或迟到的事件在不适用的状态下一律忽略，保证不会误登录。

use crate::callback::{parse_callback, CallbackError};
use crate::model::{Profile, Session};
use crate::pkce::Pkce;
use crate::protocol::TokenRequest;
use crate::provider::Provider;

/// 一次登录尝试：提供方、PKCE 对与 `state`。熵由调用方注入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt {
    pub provider: Provider,
    pub pkce: Pkce,
    pub state: String,
}

/// 交给浏览器的授权参数（不含 verifier）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizeRequest {
    pub provider: Provider,
    pub code_challenge: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailReason {
    ProviderDenied(String),
    StateMismatch,
    InvalidCallback,
    Exchange(String),
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowState {
    Idle,
    AwaitingBrowser(Attempt),
    Exchanging(Provider),
    SignedIn(Profile),
    Failed {
        provider: Provider,
        reason: FailReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowEvent {
    Begin(Attempt),
    /// 回环监听收到的请求目标，如 `/callback?code=…&state=…`。
    CallbackReceived(String),
    ExchangeSucceeded(Session),
    ExchangeFailed(String),
    Cancel,
    TimedOut,
    SignOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowAction {
    OpenBrowser(AuthorizeRequest),
    ExchangeCode(TokenRequest),
    PersistSession(Session),
    ClearSession,
}

#[derive(Debug)]
pub struct Flow {
    state: FlowState,
}

impl Default for Flow {
    fn default() -> Self {
        Self::new()
    }
}

impl Flow {
    pub fn new() -> Self {
        Self {
            state: FlowState::Idle,
        }
    }

    /// 启动时由已恢复的本地会话构造。
    pub fn signed_in(profile: Profile) -> Self {
        Self {
            state: FlowState::SignedIn(profile),
        }
    }

    pub fn state(&self) -> &FlowState {
        &self.state
    }

    pub fn handle(&mut self, event: FlowEvent) -> Option<FlowAction> {
        match (&self.state, event) {
            (
                FlowState::Idle | FlowState::Failed { .. } | FlowState::AwaitingBrowser(_),
                FlowEvent::Begin(attempt),
            ) => {
                let action = FlowAction::OpenBrowser(AuthorizeRequest {
                    provider: attempt.provider,
                    code_challenge: attempt.pkce.challenge().to_string(),
                    state: attempt.state.clone(),
                });
                self.state = FlowState::AwaitingBrowser(attempt);
                Some(action)
            }
            (FlowState::AwaitingBrowser(attempt), FlowEvent::CallbackReceived(target)) => {
                let provider = attempt.provider;
                match parse_callback(&target, &attempt.state) {
                    Ok(code) => {
                        let request = TokenRequest {
                            code,
                            code_verifier: attempt.pkce.verifier().to_string(),
                        };
                        self.state = FlowState::Exchanging(provider);
                        Some(FlowAction::ExchangeCode(request))
                    }
                    Err(error) => {
                        self.fail(provider, reason_for(error));
                        None
                    }
                }
            }
            (FlowState::AwaitingBrowser(attempt), FlowEvent::TimedOut) => {
                let provider = attempt.provider;
                self.fail(provider, FailReason::TimedOut);
                None
            }
            (FlowState::Exchanging(_), FlowEvent::ExchangeSucceeded(session)) => {
                self.state = FlowState::SignedIn(session.profile.clone());
                Some(FlowAction::PersistSession(session))
            }
            (FlowState::Exchanging(provider), FlowEvent::ExchangeFailed(message)) => {
                let provider = *provider;
                self.fail(provider, FailReason::Exchange(message));
                None
            }
            (FlowState::AwaitingBrowser(_) | FlowState::Exchanging(_), FlowEvent::Cancel) => {
                self.state = FlowState::Idle;
                None
            }
            (FlowState::SignedIn(_), FlowEvent::SignOut) => {
                self.state = FlowState::Idle;
                Some(FlowAction::ClearSession)
            }
            _ => None,
        }
    }

    fn fail(&mut self, provider: Provider, reason: FailReason) {
        self.state = FlowState::Failed { provider, reason };
    }
}

fn reason_for(error: CallbackError) -> FailReason {
    match error {
        CallbackError::StateMismatch | CallbackError::MissingState => FailReason::StateMismatch,
        CallbackError::ProviderDenied(reason) => FailReason::ProviderDenied(reason),
        CallbackError::Malformed | CallbackError::MissingCode => FailReason::InvalidCallback,
    }
}
