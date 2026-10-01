use super::ServerIdentity;
use anyhow::{ensure, Result};
use serde_json::{json, Value};

impl ServerIdentity {
    /// Sign only a fresh relay challenge, after the caller checks that the challenge audience
    /// matches its configured HTTPS endpoint. Reuses the pairing identity, not a provider key.
    pub fn relay_registration(&self, audience: &str, challenge: &str) -> Result<Value> {
        ensure!(
            audience.len() <= 256 && !audience.chars().any(char::is_control),
            "invalid relay audience"
        );
        let url = url::Url::parse(audience)?;
        ensure!(
            (url.scheme() == "https"
                || (url.scheme() == "http"
                    && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))))
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
                && url.port() != Some(0)
                && url
                    .path()
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"/_-".contains(&b)),
            "invalid relay audience"
        );
        ensure!(
            challenge.len() == 64
                && challenge
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "invalid relay challenge"
        );
        let message = format!("Norma relay registration v1\n{audience}\n{challenge}");
        Ok(json!({
            "type": "register", "machine_id": self.machine_id(),
            "signature": hex::encode(self.sign(message.as_bytes())?)
        }))
    }
}
