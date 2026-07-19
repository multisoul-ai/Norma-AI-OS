use std::fmt;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderProtocol {
    Ollama,
    OpenAiCompatible,
}

#[derive(Clone, Eq, PartialEq)]
pub struct ProviderConfig {
    protocol: ProviderProtocol,
    endpoint: String,
    model: String,
    api_key: Option<String>,
    context_window_tokens: Option<usize>,
}

impl ProviderConfig {
    pub fn new(
        protocol: ProviderProtocol,
        endpoint: impl Into<String>,
        model: impl Into<String>,
        api_key: Option<String>,
    ) -> Self {
        Self {
            protocol,
            endpoint: endpoint.into(),
            model: model.into(),
            api_key,
            context_window_tokens: None,
        }
    }

    pub fn with_context_window_tokens(mut self, context_window_tokens: usize) -> Self {
        self.context_window_tokens = Some(context_window_tokens.max(1));
        self
    }

    pub fn protocol(&self) -> ProviderProtocol {
        self.protocol
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    pub fn context_window_tokens(&self) -> Option<usize> {
        self.context_window_tokens
    }

    pub fn validate(&self) -> Result<(), ProviderError> {
        let endpoint = self.endpoint.trim();
        if endpoint.is_empty() {
            return Err(ProviderError::Message(
                "provider base_url is required".to_string(),
            ));
        }
        if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
            return Err(ProviderError::Message(
                "provider base_url must use http:// or https://".to_string(),
            ));
        }
        if self.model.trim().is_empty() {
            return Err(ProviderError::Message(
                "provider model is required".to_string(),
            ));
        }
        Ok(())
    }

    pub fn redacted(&self) -> RedactedProviderConfig {
        RedactedProviderConfig {
            protocol: self.protocol,
            endpoint: self.endpoint.clone(),
            model: self.model.clone(),
            api_key: self.api_key.as_ref().map(|_| "<redacted>".to_string()),
            context_window_tokens: self.context_window_tokens,
        }
    }
}

impl fmt::Debug for ProviderConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.redacted().fmt(formatter)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RedactedProviderConfig {
    protocol: ProviderProtocol,
    endpoint: String,
    model: String,
    api_key: Option<String>,
    context_window_tokens: Option<usize>,
}

impl fmt::Display for RedactedProviderConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "protocol={:?}, endpoint={}, model={}, api_key={}, context_window_tokens={}",
            self.protocol,
            self.endpoint,
            self.model,
            self.api_key.as_deref().unwrap_or("<empty>"),
            self.context_window_tokens
                .map(|value| value.to_string())
                .as_deref()
                .unwrap_or("<default>")
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum ProviderError {
    #[error("{0}")]
    Message(String),
}
