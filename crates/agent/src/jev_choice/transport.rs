use super::{JevChoiceRequest, JevError, JevObservation};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// No Debug: headers and request state must never appear in diagnostics.
/// This client has no tool executor and is not connected to production voice turns.
pub struct JevChoiceClient {
    client: reqwest::Client,
    endpoint: String,
}

impl JevChoiceClient {
    #[cfg(test)]
    pub(super) fn for_loopback(endpoint: String) -> Self {
        assert!(
            endpoint.starts_with("http://127.0.0.1:"),
            "测试传输只允许回环端点"
        );
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("测试客户端");
        Self { client, endpoint }
    }

    pub fn new(api_key: String) -> Result<Self, JevError> {
        if api_key.trim().is_empty() {
            return Err(JevError::InvalidRequest);
        }
        let mut value = HeaderValue::from_str(&format!("Bearer {api_key}"))
            .map_err(|_| JevError::InvalidRequest)?;
        value.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, value);
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .default_headers(headers)
            .build()
            .map_err(|_| JevError::Network)?;
        Ok(Self {
            client,
            endpoint: "https://api.typesafe.ai/v1/systemone".into(),
        })
    }

    pub async fn observe(
        &self,
        request: &JevChoiceRequest,
        timeout: Duration,
        cancellation: CancellationToken,
    ) -> Result<JevObservation, JevError> {
        if timeout.is_zero() || timeout > Duration::from_secs(30) {
            return Err(JevError::InvalidRequest);
        }
        let work = async {
            let mut response = self
                .client
                .post(&self.endpoint)
                .json(&request.body())
                .send()
                .await
                .map_err(|_| JevError::Network)?;
            if !response.status().is_success() {
                return Err(JevError::Http {
                    status: response.status().as_u16(),
                });
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|_| JevError::Network)? {
                if bytes.len() + chunk.len() > 262_144 {
                    return Err(JevError::InvalidResponse);
                }
                bytes.extend_from_slice(&chunk);
            }
            let value = serde_json::from_slice(&bytes).map_err(|_| JevError::InvalidResponse)?;
            request.decode(value)
        };
        let result = tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(JevError::Cancelled),
            result = tokio::time::timeout(timeout, work) => result.map_err(|_| JevError::Timeout)?,
        };
        if cancellation.is_cancelled() {
            return Err(JevError::Cancelled);
        }
        result
    }
}
