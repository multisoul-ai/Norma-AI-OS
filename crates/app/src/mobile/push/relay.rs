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
        let path = std::env::var_os("NORMA_PUSH_RELAY_KEY_FILE")
            .context("Missing notification service token file")?;
        let file = std::fs::File::open(Path::new(&path))
            .context("Unable to read notification service token")?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file()
                && metadata.permissions().mode() & 0o077 == 0
                && metadata.len() <= 1024,
            "The token file must be a regular file of at most 1 KiB, accessible only by its owner."
        );
        let mut key = String::new();
        file.take(1025).read_to_string(&mut key)?;
        let environment = match std::env::var("NORMA_PUSH_ENVIRONMENT").as_deref() {
            Ok("sandbox") => PushEnvironment::Sandbox,
            Ok("production") => PushEnvironment::Production,
            _ => anyhow::bail!("Set the notification environment to sandbox or production"),
        };
        Self::new(
            endpoint
                .to_str()
                .context("Invalid notification service URL")?,
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
        ensure!(
            endpoint.len() <= 2048,
            "Notification service URL is too long"
        );
        let endpoint = url::Url::parse(endpoint).context("Invalid notification service URL")?;
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
                && matches!(
                    endpoint.path(),
                    "/v1/notifications" | "/norma/v1/notifications"
                ),
            "Use an HTTPS URL with /v1/notifications or /norma/v1/notifications"
        );
        ensure!(
            key.len() == 64
                && key
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "Invalid notification service token format"
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
        if matches!(status, 401 | 403 | 422) {
            return Err("push_configuration_error");
        }
        let body: Value = response
            .body_mut()
            .with_config()
            .limit(4096)
            .read_json()
            .map_err(|_| "delivery_unknown")?;
        // Only fixed protocol codes may reach the log; never echo a response body or credential.
        match (status, body["error"].as_str()) {
            (200, _) if body["status"] == "accepted" => Ok(None),
            (410, Some("token_unregistered")) => body["invalidated_at"]
                .as_u64()
                .filter(|time| *time <= i64::MAX as u64)
                .map(Some)
                .ok_or("delivery_unknown"),
            (400, Some("invalid_event")) => Err("invalid_event"),
            (400, Some("event_expired")) => Err("event_expired"),
            (409, Some("event_conflict")) => Err("event_conflict"),
            (413, _) => Err("body_too_large"),
            (502, Some("apns_configuration_error")) => Err("apns_configuration_error"),
            (502, Some("apns_rejected")) => Err("apns_rejected"),
            (503, Some("apns_unavailable")) => Err("apns_unavailable"),
            (503, Some("relay_busy")) => Err("relay_busy"),
            (503, Some("push_unavailable")) => Err("push_unavailable"),
            _ => Err("delivery_unknown"),
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/mobile_push_config_tests.rs"]
mod tests;
