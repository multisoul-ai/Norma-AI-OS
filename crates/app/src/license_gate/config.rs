//! License client defaults: embedded files + optional env overrides.
//!
//! **换默认 URL / 公钥：编辑 `embed/` 目录下的文本文件**（见 `embed/README.md`）。
//! 勿把签发私钥放进仓库。

pub const SKIP_LICENSE_ENV: &str = "NORMA_SKIP_LICENSE";
pub const LICENSE_URL_ENV: &str = "NORMA_LICENSE_URL";
pub const LICENSE_PUBKEY_ENV: &str = "NORMA_LICENSE_PUBKEY_HEX";
/// Build-time rollout switch. Default and packaged builds leave this disabled.
pub const LICENSES_ENABLED: bool = cfg!(feature = "licenses");

/// Maintainer-editable defaults shipped in the binary (`embed/`).
const EMBEDDED_BASE_URL: &str = include_str!("embed/base_url.txt");
const EMBEDDED_PUBKEY_HEX: &str = include_str!("embed/public_key.hex");

pub fn skip_license_enabled() -> bool {
    parse_truthy_env(std::env::var_os(SKIP_LICENSE_ENV).and_then(|v| v.into_string().ok()))
}

/// Runtime `NORMA_LICENSE_URL` > compile-time `option_env!` > `embed/base_url.txt`.
pub fn license_base_url() -> String {
    if let Ok(value) = std::env::var(LICENSE_URL_ENV) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.trim_end_matches('/').to_string();
        }
    }
    if let Some(value) = option_env!("NORMA_LICENSE_URL") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.trim_end_matches('/').to_string();
        }
    }
    normalize_embedded_line(EMBEDDED_BASE_URL)
        .trim_end_matches('/')
        .to_string()
}

/// Runtime `NORMA_LICENSE_PUBKEY_HEX` > compile-time `option_env!` > `embed/public_key.hex`.
pub fn public_key_hex() -> String {
    if let Ok(value) = std::env::var(LICENSE_PUBKEY_ENV) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if let Some(value) = option_env!("NORMA_LICENSE_PUBKEY_HEX") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    normalize_embedded_line(EMBEDDED_PUBKEY_HEX)
}

fn normalize_embedded_line(raw: &str) -> String {
    raw.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .unwrap_or("")
        .to_string()
}

pub fn parse_truthy_env(raw: Option<String>) -> bool {
    matches!(
        raw.as_deref().map(str::trim),
        Some("1")
            | Some("true")
            | Some("TRUE")
            | Some("yes")
            | Some("YES")
            | Some("on")
            | Some("ON")
    )
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_embedded_line, parse_truthy_env, public_key_hex, EMBEDDED_BASE_URL,
        EMBEDDED_PUBKEY_HEX,
    };

    #[test]
    fn test_truthy_env_parsing() {
        assert!(parse_truthy_env(Some("1".into())), "1 应为真");
        assert!(parse_truthy_env(Some(" true ".into())), "true 应为真");
        assert!(!parse_truthy_env(Some("0".into())), "0 应为假");
        assert!(!parse_truthy_env(None), "缺失应为假");
        assert!(!parse_truthy_env(Some("".into())), "空串应为假");
    }

    /// embed/public_key.hex 必须是可维护的 64 hex，避免空公钥导致「激活成功本地校验失败」。
    #[test]
    fn test_embedded_pubkey_is_64_hex() {
        let key = normalize_embedded_line(EMBEDDED_PUBKEY_HEX);
        assert_eq!(
            key.len(),
            64,
            "embed/public_key.hex 必须正好 64 个 hex 字符"
        );
        assert!(
            key.chars().all(|c| c.is_ascii_hexdigit()),
            "embed/public_key.hex 只能包含 0-9a-fA-F"
        );
        // When runtime env is unset in unit tests, public_key_hex should resolve to embed.
        if std::env::var_os(super::LICENSE_PUBKEY_ENV).is_none() {
            assert_eq!(
                public_key_hex(),
                key,
                "无 NORMA_LICENSE_PUBKEY_HEX 时应回落到嵌入公钥"
            );
        }
    }

    #[test]
    fn test_embedded_base_url_nonempty() {
        let url = normalize_embedded_line(EMBEDDED_BASE_URL);
        assert!(!url.is_empty(), "embed/base_url.txt 不能为空");
        assert!(
            url.starts_with("http://") || url.starts_with("https://"),
            "embed/base_url.txt 应以 http(s):// 开头，当前={url}"
        );
    }

    /// 生产 Keygate 默认地址：验证发行包不需环境变量即可连接已部署服务。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   EMBEDDED_BASE_URL = embed/base_url.txt 的首个非空、非注释行
    ///   expected          = https://soulvoiceai.com/keygate
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 解析嵌入文本 → 得到发行包的 Keygate 默认 Base URL
    ///   2. 与线上部署根路径比较 → 确保 activate 会请求 `/keygate/api/v1/license/activate`
    ///
    /// 预期结果：
    ///   - 正断言：默认地址必须等于线上 Keygate 根路径
    ///   - 负验收：不得继续指向 localhost 或缺少 `/keygate` 前缀
    #[test]
    fn test_embedded_base_url_points_to_production_keygate() {
        let url = normalize_embedded_line(EMBEDDED_BASE_URL);
        assert_eq!(
            url, "https://soulvoiceai.com/keygate",
            "embed/base_url.txt 必须指向已部署的生产 Keygate，不得保留 localhost 或丢失 /keygate 前缀"
        );
    }
}
