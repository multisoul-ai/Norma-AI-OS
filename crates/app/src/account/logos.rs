//! 账号相关图标：提供方品牌图标（彩色，需用 `img()` 渲染）与设置导航图标（随二进制嵌入）。

use account::Provider;

/// 设置侧栏「账号」导航图标。
pub(crate) const ACCOUNT_NAV_ASSET: &str = "icons/settings-nav-account.svg";
const ACCOUNT_NAV_BYTES: &[u8] = include_bytes!("../../assets/icons/settings-nav-account.svg");
const GITHUB_LOGO_ASSET: &str = "icons/provider-github.svg";
const GOOGLE_LOGO_ASSET: &str = "icons/provider-google.svg";
const GITHUB_LOGO_BYTES: &[u8] = include_bytes!("../../assets/icons/provider-github.svg");
const GOOGLE_LOGO_BYTES: &[u8] = include_bytes!("../../assets/icons/provider-google.svg");

pub(crate) const fn logo_asset(provider: Provider) -> &'static str {
    match provider {
        Provider::GitHub => GITHUB_LOGO_ASSET,
        Provider::Google => GOOGLE_LOGO_ASSET,
    }
}

/// 供 `EmbeddedAssets` 解析账号相关图标；其它路径返回 `None`。
pub(crate) fn account_asset(path: &str) -> Option<&'static [u8]> {
    match path {
        ACCOUNT_NAV_ASSET => Some(ACCOUNT_NAV_BYTES),
        GITHUB_LOGO_ASSET => Some(GITHUB_LOGO_BYTES),
        GOOGLE_LOGO_ASSET => Some(GOOGLE_LOGO_BYTES),
        _ => None,
    }
}
