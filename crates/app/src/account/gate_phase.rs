//! 启动登录页的纯决策：是否显示，由调用方决定放在 License 之后、工作台之前。

use crate::license_gate::config_truthy;

pub const SKIP_LOGIN_ENV: &str = "NORMA_SKIP_LOGIN";

pub const LOGIN_ENABLE_ENV: &str = "NORMA_ACCOUNT_LOGIN";
/// 仅调试构建生效：真实 App 验收时不打开系统浏览器。
pub const NO_BROWSER_ENV: &str = "NORMA_E2E_NO_BROWSER";

/// 发布开关：默认关闭，待 norma-cloud 配好 OAuth 凭据后以 `account-login` feature 开启。
/// 调试构建还可用 `NORMA_ACCOUNT_LOGIN=1` 在运行时开启，便于开发与真实 App 验收；发行构建忽略该变量。
pub fn account_login_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        login_enabled_from(
            cfg!(feature = "account-login"),
            cfg!(debug_assertions),
            std::env::var(LOGIN_ENABLE_ENV).ok(),
        )
    })
}

pub(super) fn login_enabled_from(feature: bool, debug: bool, env: Option<String>) -> bool {
    feature || (debug && config_truthy(env))
}

/// 调试构建下是否应跳过打开系统浏览器（授权链接仍可复制）。
pub fn browser_launch_suppressed() -> bool {
    cfg!(debug_assertions) && config_truthy(std::env::var(NO_BROWSER_ENV).ok())
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
