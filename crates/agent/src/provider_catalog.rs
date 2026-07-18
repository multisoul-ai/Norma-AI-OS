use crate::{
    AgentMessage, AgentProvider, ConfiguredProvider, MessageRole, ProviderConfig, ProviderProtocol,
    ProviderRequest,
};
use std::fmt;
use std::time::Duration;

const CATALOG_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderCatalogError {
    Connection(String),
    ModelListUnavailable(String),
}

impl ProviderCatalogError {
    pub fn allows_manual_model(&self) -> bool {
        matches!(self, Self::ModelListUnavailable(_))
    }
}

impl fmt::Display for ProviderCatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connection(message) | Self::ModelListUnavailable(message) => {
                formatter.write_str(message)
            }
        }
    }
}

impl std::error::Error for ProviderCatalogError {}

pub fn discover_provider_models(
    protocol: ProviderProtocol,
    base_url: &str,
    api_key: Option<&str>,
) -> Result<Vec<String>, ProviderCatalogError> {
    let endpoint = match protocol {
        ProviderProtocol::OpenAiCompatible => {
            format!("{}/models", base_url.trim_end_matches('/'))
        }
        ProviderProtocol::Ollama => format!("{}/api/tags", base_url.trim_end_matches('/')),
    };
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(CATALOG_TIMEOUT))
        .build()
        .new_agent();
    let mut request = agent.get(&endpoint);
    if let Some(api_key) = api_key.filter(|key| !key.trim().is_empty()) {
        request = request.header("Authorization", &format!("Bearer {api_key}"));
    }
    let mut response = request.call().map_err(classify_catalog_request_error)?;
    let payload: serde_json::Value = response.body_mut().read_json().map_err(|error| {
        ProviderCatalogError::ModelListUnavailable(format!(
            "Provider model list could not be read: {error}"
        ))
    })?;
    normalize_provider_models(&payload)
}

pub fn normalize_provider_models(
    payload: &serde_json::Value,
) -> Result<Vec<String>, ProviderCatalogError> {
    let openai = payload.get("data").and_then(serde_json::Value::as_array);
    let ollama = payload.get("models").and_then(serde_json::Value::as_array);
    let models = openai.or(ollama).ok_or_else(|| {
        ProviderCatalogError::ModelListUnavailable(
            "Provider response did not contain a model list.".to_string(),
        )
    })?;
    let mut ids = models
        .iter()
        .filter_map(|model| {
            model
                .get("id")
                .or_else(|| model.get("name"))
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(ToOwned::to_owned)
        })
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    (!ids.is_empty()).then_some(ids).ok_or_else(|| {
        ProviderCatalogError::ModelListUnavailable(
            "Provider returned no selectable models.".to_string(),
        )
    })
}

pub fn validate_provider_model(config: ProviderConfig) -> Result<(), String> {
    let mut provider = ConfiguredProvider::new(config).map_err(|error| error.to_string())?;
    provider
        .complete(&ProviderRequest {
            messages: vec![AgentMessage {
                role: MessageRole::User,
                content: "Reply with OK.".to_string(),
                tool_call_id: None,
                tool_calls: Vec::new(),
            }],
            tools: Vec::new(),
        })
        .map(|_| ())
        .map_err(|error| format!("Could not validate the selected model: {error}"))
}

fn classify_catalog_request_error(error: ureq::Error) -> ProviderCatalogError {
    match error {
        ureq::Error::StatusCode(404 | 405 | 501) => ProviderCatalogError::ModelListUnavailable(
            format!("Provider does not expose a model list endpoint: {error}"),
        ),
        _ => ProviderCatalogError::Connection(format!("Could not connect to provider: {error}")),
    }
}
