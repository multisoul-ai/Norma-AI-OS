//! 账号与会话模型。身份由 `(provider, subject)` 决定，邮箱只作展示。

use crate::provider::Provider;
use serde::{Deserialize, Serialize};
use std::fmt;

/// 会话持久化格式版本；读到更高版本时按未登录处理。
pub const SESSION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AccountId(String);

impl AccountId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub provider: Provider,
    pub subject: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub account_id: AccountId,
    pub display_name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub identities: Vec<Identity>,
}

impl Profile {
    /// 登录来源：身份列表的第一项。
    pub fn primary_provider(&self) -> Option<Provider> {
        self.identities.first().map(|i| i.provider)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionDecodeError {
    Malformed,
    UnsupportedVersion(u32),
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub schema_version: u32,
    pub access_token: String,
    pub refresh_token: String,
    /// Unix 秒。
    pub expires_at: u64,
    pub profile: Profile,
}

impl Session {
    pub fn new(
        access_token: String,
        refresh_token: String,
        expires_at: u64,
        profile: Profile,
    ) -> Self {
        Self {
            schema_version: SESSION_SCHEMA_VERSION,
            access_token,
            refresh_token,
            expires_at,
            profile,
        }
    }

    /// 到点即过期。
    pub fn is_expired(&self, now: u64) -> bool {
        now >= self.expires_at
    }

    pub fn encode(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn decode(raw: &str) -> Result<Self, SessionDecodeError> {
        #[derive(Deserialize)]
        struct Header {
            schema_version: u32,
        }
        let header: Header =
            serde_json::from_str(raw).map_err(|_| SessionDecodeError::Malformed)?;
        if header.schema_version > SESSION_SCHEMA_VERSION {
            return Err(SessionDecodeError::UnsupportedVersion(
                header.schema_version,
            ));
        }
        serde_json::from_str(raw).map_err(|_| SessionDecodeError::Malformed)
    }
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("schema_version", &self.schema_version)
            .field("access_token", &"<redacted>")
            .field("refresh_token", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .field("profile", &self.profile)
            .finish()
    }
}
