//! norma-cloud `/v1/auth` 的线上类型与授权地址构造。

use crate::flow::AuthorizeRequest;
use crate::model::{Profile, Session, SessionDecodeError, SESSION_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};

pub const API_PREFIX: &str = "/v1/auth";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenRequest {
    pub code: String,
    pub code_verifier: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenResponse {
    pub schema_version: u32,
    pub access_token: String,
    pub refresh_token: String,
    /// 有效秒数。
    pub expires_in: u64,
    pub profile: Profile,
}

impl TokenResponse {
    pub fn into_session(self, now: u64) -> Result<Session, SessionDecodeError> {
        if self.schema_version > SESSION_SCHEMA_VERSION {
            return Err(SessionDecodeError::UnsupportedVersion(self.schema_version));
        }
        Ok(Session::new(
            self.access_token,
            self.refresh_token,
            now.saturating_add(self.expires_in),
            self.profile,
        ))
    }
}

/// 浏览器要打开的授权开始地址。
pub fn start_url(base: &str, request: &AuthorizeRequest, redirect_port: u16) -> String {
    format!(
        "{}{}/{}/start?code_challenge={}&state={}&redirect_port={}",
        base.trim_end_matches('/'),
        API_PREFIX,
        request.provider.id(),
        encode_component(&request.code_challenge),
        encode_component(&request.state),
        redirect_port
    )
}

fn encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
