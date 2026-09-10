use anyhow::{ensure, Context, Result};
use serde_json::Value;
use std::{io::Read, os::unix::fs::PermissionsExt, path::Path, time::Duration};
use storage::PushEnvironment;

/// Private deployment configuration; never store the Apple signing key on the Mac.
pub struct PushRelayConfig {
    pub(super) endpoint: url::Url,
    pub(super) key: String,
    pub(super) environment: PushEnvironment,
}
impl PushRelayConfig {
    pub(crate) fn from_fields(
        endpoint: &str,
        key: &str,
        environment: PushEnvironment,
    ) -> Result<Self> {
        Self::new(endpoint.trim(), key.trim(), environment, false)
    }
    pub fn from_environment() -> Result<Option<Self>> {
        let Some(endpoint) = std::env::var_os("NORMA_PUSH_RELAY_URL") else {
            return Ok(None);
        };
        let path = std::env::var_os("NORMA_PUSH_RELAY_KEY_FILE").context("缺少通知服务凭据文件")?;
        let file = std::fs::File::open(Path::new(&path)).context("无法读取通知服务凭据")?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file()
                && metadata.permissions().mode() & 0o077 == 0
                && metadata.len() <= 1024,
            "通知服务凭据文件须为仅自己可读的小文件"
        );
        let mut key = String::new();
        file.take(1025).read_to_string(&mut key)?;
        let environment = match std::env::var("NORMA_PUSH_ENVIRONMENT").as_deref() {
            Ok("sandbox") => PushEnvironment::Sandbox,
            Ok("production") => PushEnvironment::Production,
            _ => anyhow::bail!("通知服务须指定sandbox或production"),
        };
        Self::new(
            endpoint.to_str().context("无效通知服务地址")?,
            key.trim(),
            environment,
            false,
        )
        .map(Some)
    }

    fn new(
        endpoint: &str,
        key: &str,
        environment: PushEnvironment,
        loopback: bool,
    ) -> Result<Self> {
        ensure!(endpoint.len() <= 2048, "通知服务地址过长");
        let endpoint = url::Url::parse(endpoint).context("无效通知服务地址")?;
        let test_http = loopback
            && endpoint.scheme() == "http"
            && endpoint.host_str().is_some_and(|host| {
                host.parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
            });
        ensure!(
            (endpoint.scheme() == "https" || test_http)
                && endpoint.host_str().is_some()
                && endpoint.username().is_empty()
                && endpoint.password().is_none()
                && endpoint.query().is_none()
                && endpoint.fragment().is_none()
                && endpoint.path() == "/v1/notifications",
            "通知服务需要HTTPS /v1/notifications地址"
        );
        ensure!(
            key.len() == 64
                && key
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "通知服务凭据格式无效"
        );
        Ok(Self {
            endpoint,
            key: key.into(),
            environment,
        })
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn for_e2e(endpoint: &str, key: &str, environment: PushEnvironment) -> Result<Self> {
        Self::new(endpoint, key, environment, true)
    }
}

pub(super) struct Relay {
    pub config: PushRelayConfig,
    agent: ureq::Agent,
}
impl Relay {
    pub fn new(config: PushRelayConfig) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            // 12s = relay's 10s APNs wait + 2s network margin. No retry of a POST with unknown outcome.
            .timeout_global(Some(Duration::from_secs(12)))
            .build()
            .new_agent();
        Self { config, agent }
    }
    pub fn send(&self, event: &Value) -> Result<Option<u64>, &'static str> {
        let body = serde_json::to_vec(event).map_err(|_| "invalid_event")?;
        if body.len() > 4096 {
            return Err("invalid_event");
        }
        let mut response = self
            .agent
            .post(self.config.endpoint.as_str())
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.config.key))
            .send(body)
            .map_err(|_| "delivery_unknown")?;
        let status = response.status().as_u16();
        let body: Value = response
            .body_mut()
            .with_config()
            .limit(4096)
            .read_json()
            .map_err(|_| "delivery_unknown")?;
        match status {
            200 if body["status"] == "accepted" => Ok(None),
            410 if body["error"] == "token_unregistered" => body["invalidated_at"]
                .as_u64()
                .filter(|time| *time <= i64::MAX as u64)
                .map(Some)
                .ok_or("delivery_unknown"),
            401 | 403 | 422 => Err("push_configuration_error"),
            _ => Err("delivery_unknown"),
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/mobile_push_config_tests.rs"]
mod tests;
