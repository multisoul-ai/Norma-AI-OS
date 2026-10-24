//! 账号域层：登录状态机、PKCE、会话模型与 norma-cloud `/v1` 协议类型。
//!
//! 本 crate 不依赖 GPUI、Storage、UI 或网络实现；副作用通过
//! [`AccountSessionStore`] 与 [`AuthApi`] 两个端口交给 `app` 层。

mod callback;
mod flow;
mod model;
mod pkce;
mod ports;
mod protocol;
mod provider;

pub use callback::{parse_callback, CallbackError};
pub use flow::{Attempt, AuthorizeRequest, FailReason, Flow, FlowAction, FlowEvent, FlowState};
pub use model::{
    AccountId, Identity, Profile, Session, SessionDecodeError, SESSION_SCHEMA_VERSION,
};
pub use pkce::{challenge_for, random_state, verify_state, Pkce};
pub use ports::{AccountSessionStore, AuthApi, AuthError, StoreError};
pub use protocol::{start_url, TokenRequest, TokenResponse, API_PREFIX};
pub use provider::Provider;

#[cfg(test)]
#[path = "../tests/unit/callback_tests.rs"]
mod callback_tests;
#[cfg(test)]
#[path = "../tests/unit/fixtures.rs"]
mod fixtures;
#[cfg(test)]
#[path = "../tests/unit/flow_tests.rs"]
mod flow_tests;
#[cfg(test)]
#[path = "../tests/unit/model_tests.rs"]
mod model_tests;
#[cfg(test)]
#[path = "../tests/unit/pkce_tests.rs"]
mod pkce_tests;
#[cfg(test)]
#[path = "../tests/unit/protocol_tests.rs"]
mod protocol_tests;
#[cfg(test)]
#[path = "../tests/unit/provider_tests.rs"]
mod provider_tests;
