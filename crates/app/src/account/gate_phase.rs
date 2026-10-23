//! 启动登录页的纯决策：是否显示，由调用方决定放在 License 之后、工作台之前。

use crate::license_gate::config_truthy;

pub const SKIP_LOGIN_ENV: &str = "NORMA_SKIP_LOGIN";

/// 发布开关：默认关闭，待 norma-cloud 配好 OAuth 凭据后以 `account-login` feature 开启。
pub const fn account_login_enabled() -> bool {
    cfg!(feature = "account-login")
}

pub fn skip_login_requested() -> bool {
    parse_skip_login(std::env::var(SKIP_LOGIN_ENV).ok())
}

pub(super) fn parse_skip_login(raw: Option<String>) -> bool {
    config_truthy(raw)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoginGateInputs {
    pub enabled: bool,
    pub skip: bool,
    pub has_session: bool,
    pub dismissed: bool,
}

pub fn should_show_login(inputs: LoginGateInputs) -> bool {
    inputs.enabled && !inputs.skip && !inputs.has_session && !inputs.dismissed
}
