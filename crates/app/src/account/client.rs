//! norma-cloud `/v1/auth` 的同步 HTTP 客户端，实现 `AuthApi`；必须在后台线程调用。

use account::{AuthApi, AuthError, Profile, Session, TokenRequest, TokenResponse, API_PREFIX};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(crate) const CLOUD_URL_ENV: &str = "NORMA_CLOUD_URL";
const DEFAULT_CLOUD_URL: &str = "https://soulvoiceai.com/norma";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// 运行时 `NORMA_CLOUD_URL` > 内置默认地址（不暴露在设置页）。
pub(crate) fn account_base_url() -> String {
    std::env::var(CLOUD_URL_ENV)
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_CLOUD_URL.to_string())
}

#[derive(Clone, Debug)]
pub(crate) struct CloudAuthApi {
    base_url: String,
}

impl CloudAuthApi {
    pub(crate) fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{API_PREFIX}/{path}", self.base_url)
    }

    fn agent() -> ureq::Agent {
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(REQUEST_TIMEOUT))
            .build()
            .new_agent()
    }

    fn read(response: ureq::http::Response<ureq::Body>) -> Result<(u16, String), AuthError> {
        let status = response.status().as_u16();
        let body = response
            .into_body()
            .read_to_string()
            .map_err(|error| AuthError::Network(error.to_string()))?;
        Ok((status, body))
    }

    fn session_from(status: u16, body: &str) -> Result<Session, AuthError> {
        check_status(status, body)?;
        let response: TokenResponse = serde_json::from_str(body)
            .map_err(|error| AuthError::Unsupported(error.to_string()))?;
        response
            .into_session(now_unix())
            .map_err(|error| AuthError::Unsupported(format!("{error:?}")))
    }
}

impl AuthApi for CloudAuthApi {
    fn exchange(&self, request: &TokenRequest) -> Result<Session, AuthError> {
        let response = Self::agent()
            .post(&self.url("token"))
            .send_json(request)
            .map_err(|error| AuthError::Network(error.to_string()))?;
        let (status, body) = Self::read(response)?;
        Self::session_from(status, &body)
    }

    fn refresh(&self, refresh_token: &str) -> Result<Session, AuthError> {
        let response = Self::agent()
            .post(&self.url("refresh"))
            .send_json(serde_json::json!({ "refresh_token": refresh_token }))
            .map_err(|error| AuthError::Network(error.to_string()))?;
        let (status, body) = Self::read(response)?;
        Self::session_from(status, &body)
    }

    fn me(&self, access_token: &str) -> Result<Profile, AuthError> {
        let response = Self::agent()
            .get(&self.url("me"))
            .header("Authorization", &format!("Bearer {access_token}"))
            .call()
            .map_err(|error| AuthError::Network(error.to_string()))?;
        let (status, body) = Self::read(response)?;
        check_status(status, &body)?;
        serde_json::from_str(&body).map_err(|error| AuthError::Unsupported(error.to_string()))
    }

    fn logout(&self, access_token: &str) -> Result<(), AuthError> {
        let response = Self::agent()
            .post(&self.url("logout"))
            .header("Authorization", &format!("Bearer {access_token}"))
            .send_empty()
            .map_err(|error| AuthError::Network(error.to_string()))?;
        let (status, body) = Self::read(response)?;
        check_status(status, &body)
    }
}

fn check_status(status: u16, body: &str) -> Result<(), AuthError> {
    match status {
        200..=299 => Ok(()),
        401 => Err(AuthError::Unauthorized),
        _ => {
            let code = serde_json::from_str::<serde_json::Value>(body)
                .ok()
                .and_then(|value| {
                    value
                        .get("error")
                        .and_then(|e| e.as_str().map(str::to_string))
                })
                .unwrap_or_else(|| format!("http_{status}"));
            Err(AuthError::Rejected(code))
        }
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
