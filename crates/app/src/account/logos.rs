//! 登录提供方品牌图标（随二进制嵌入；彩色，需用 `img()` 渲染）。

use account::Provider;

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

/// 供 `EmbeddedAssets` 解析提供方图标；非提供方图标返回 `None`。
pub(crate) fn provider_logo(path: &str) -> Option<&'static [u8]> {
    match path {
        GITHUB_LOGO_ASSET => Some(GITHUB_LOGO_BYTES),
        GOOGLE_LOGO_ASSET => Some(GOOGLE_LOGO_BYTES),
        _ => None,
    }
}
