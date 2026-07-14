//! Env / compile-time configuration for Keygate client wiring.

pub const SKIP_LICENSE_ENV: &str = "NORMA_SKIP_LICENSE";
pub const LICENSE_URL_ENV: &str = "NORMA_LICENSE_URL";
pub const LICENSE_PUBKEY_ENV: &str = "NORMA_LICENSE_PUBKEY_HEX";

/// Default Keygate base URL (local spike). Override with `NORMA_LICENSE_URL`.
pub const DEFAULT_LICENSE_BASE_URL: &str = "http://localhost:9000";

/// Compile-time embedded verifying key (64 hex chars). Empty until release wiring.
const EMBEDDED_PUBKEY_HEX: &str = match option_env!("NORMA_LICENSE_PUBKEY_HEX") {
    Some(value) => value,
    None => "",
};

pub fn skip_license_enabled() -> bool {
    parse_truthy_env(std::env::var_os(SKIP_LICENSE_ENV).and_then(|v| v.into_string().ok()))
}

pub fn license_base_url() -> String {
    std::env::var(LICENSE_URL_ENV).unwrap_or_else(|_| DEFAULT_LICENSE_BASE_URL.to_string())
}

/// Prefer runtime env (spike/dev), then compile-time embed.
pub fn public_key_hex() -> String {
    if let Ok(value) = std::env::var(LICENSE_PUBKEY_ENV) {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }
    EMBEDDED_PUBKEY_HEX.trim().to_string()
}

pub fn parse_truthy_env(raw: Option<String>) -> bool {
    match raw.as_deref().map(str::trim) {
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES") | Some("on")
        | Some("ON") => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_truthy_env;

    #[test]
    fn test_truthy_env_parsing() {
        assert!(parse_truthy_env(Some("1".into())), "1 应为真");
        assert!(parse_truthy_env(Some(" true ".into())), "true 应为真");
        assert!(!parse_truthy_env(Some("0".into())), "0 应为假");
        assert!(!parse_truthy_env(None), "缺失应为假");
        assert!(!parse_truthy_env(Some("".into())), "空串应为假");
    }
}
