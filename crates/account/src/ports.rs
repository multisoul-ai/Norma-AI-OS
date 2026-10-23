//! 副作用端口：`app` 层提供真实实现，测试使用内存假实现。

use crate::model::{Profile, Session};
use crate::protocol::TokenRequest;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreError(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    Network(String),
    Rejected(String),
    Unauthorized,
    Unsupported(String),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network(m) => write!(f, "network error: {m}"),
            Self::Rejected(m) => write!(f, "rejected: {m}"),
            Self::Unauthorized => f.write_str("unauthorized"),
            Self::Unsupported(m) => write!(f, "unsupported: {m}"),
        }
    }
}

/// 会话与「已跳过登录页」标记的持久化。读取失败应按未登录处理，故 `load` 不返回错误。
pub trait AccountSessionStore {
    fn load(&self) -> Option<Session>;
    fn save(&self, session: &Session) -> Result<(), StoreError>;
    fn clear(&self) -> Result<(), StoreError>;
    fn login_prompt_dismissed(&self) -> bool;
    fn set_login_prompt_dismissed(&self, dismissed: bool) -> Result<(), StoreError>;
}

/// norma-cloud 的同步调用；由 `app` 在后台线程执行。
pub trait AuthApi {
    fn exchange(&self, request: &TokenRequest) -> Result<Session, AuthError>;
    fn refresh(&self, refresh_token: &str) -> Result<Session, AuthError>;
    fn me(&self, access_token: &str) -> Result<Profile, AuthError>;
    fn logout(&self, access_token: &str) -> Result<(), AuthError>;
}
