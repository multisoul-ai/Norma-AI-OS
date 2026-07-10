use crate::{ProviderError, ProviderRequest, ProviderResponse, ProviderToolCall};
use serde::Deserialize;
use serde_json::{json, Value};
use std::fmt;

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
        }
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
        }
    }
}

impl fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.redacted().fmt(f)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RedactedProviderConfig {
    protocol: ProviderProtocol,
    endpoint: String,
    model: String,
    api_key: Option<String>,
}

impl fmt::Display for RedactedProviderConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "protocol={:?}, endpoint={}, model={}, api_key={}",
            self.protocol,
            self.endpoint,
            self.model,
            self.api_key.as_deref().unwrap_or("<empty>")
        )
    }
}

pub fn build_ollama_chat_request(
    config: &ProviderConfig,
    request: &ProviderRequest,
) -> Result<String, ProviderError> {
    ensure_protocol(config, ProviderProtocol::Ollama)?;
    let body = json!({
        "model": config.model,
        "messages": messages_json(request, false),
        "tools": tools_json(request),
        "stream": false,
        "think": false,
    });
    build_http_request(config, "/api/chat", &body, false)
}

pub fn build_openai_chat_request(
    config: &ProviderConfig,
    request: &ProviderRequest,
) -> Result<String, ProviderError> {
    ensure_protocol(config, ProviderProtocol::OpenAiCompatible)?;
    let body = json!({
        "model": config.model,
        "messages": messages_json(request, true),
        "tools": tools_json(request),
        "stream": false,
    });
    build_http_request(config, "/chat/completions", &body, true)
}

pub fn parse_openai_response(raw: &str) -> Result<ProviderResponse, ProviderError> {
    let value: OpenAiResponse = serde_json::from_str(raw)
        .map_err(|error| ProviderError::Message(format!("OpenAI response JSON failed: {error}")))?;
    let choice = value
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| ProviderError::Message("OpenAI response has no choices".to_string()))?;
    let tool_calls = choice
        .message
        .tool_calls
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(index, call)| ProviderToolCall {
            id: call
                .id
                .unwrap_or_else(|| format!("openai-tool-call-{index}")),
            name: call.function.name,
            arguments_json: call.function.arguments,
        })
        .collect();
    Ok(ProviderResponse {
        text: choice.message.content.unwrap_or_default(),
        tool_calls,
    })
}

pub fn parse_ollama_response(raw: &str) -> Result<ProviderResponse, ProviderError> {
    let value: OllamaResponse = serde_json::from_str(raw)
        .map_err(|error| ProviderError::Message(format!("Ollama response JSON failed: {error}")))?;
    let tool_calls = value
        .message
        .tool_calls
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(index, call)| {
            let arguments_json = match call.function.arguments {
                Value::String(value) => value,
                value => serde_json::to_string(&value).map_err(|error| {
                    ProviderError::Message(format!("Ollama tool arguments JSON failed: {error}"))
                })?,
            };
            Ok(ProviderToolCall {
                id: call
                    .id
                    .unwrap_or_else(|| format!("ollama-tool-call-{index}")),
                name: call.function.name,
                arguments_json,
            })
        })
        .collect::<Result<Vec<_>, ProviderError>>()?;
    Ok(ProviderResponse {
        text: value.message.content.unwrap_or_default(),
        tool_calls,
    })
}

fn ensure_protocol(
    config: &ProviderConfig,
    expected: ProviderProtocol,
) -> Result<(), ProviderError> {
    if config.protocol != expected {
        return Err(ProviderError::Message(
            "provider protocol does not match request codec".to_string(),
        ));
    }
    config.validate()
}

fn build_http_request(
    config: &ProviderConfig,
    path: &str,
    body: &Value,
    bearer: bool,
) -> Result<String, ProviderError> {
    let body = serde_json::to_string(body).map_err(|error| {
        ProviderError::Message(format!("provider request JSON failed: {error}"))
    })?;
    let endpoint = config
        .endpoint
        .strip_prefix("http://")
        .or_else(|| config.endpoint.strip_prefix("https://"))
        .ok_or_else(|| {
            ProviderError::Message("provider base_url must use http:// or https://".into())
        })?
        .trim_end_matches('/');
    let mut endpoint_parts = endpoint.splitn(2, '/');
    let host = endpoint_parts.next().unwrap_or_default();
    let prefix = endpoint_parts
        .next()
        .unwrap_or_default()
        .trim_end_matches('/');
    let path = if prefix.is_empty() {
        path.to_string()
    } else {
        format!("/{prefix}{path}")
    };
    let authorization = if bearer {
        config
            .api_key
            .as_deref()
            .map(|key| format!("Authorization: Bearer {key}\r\n"))
            .unwrap_or_default()
    } else {
        String::new()
    };
    Ok(format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\n{authorization}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    ))
}

fn messages_json(request: &ProviderRequest, openai: bool) -> Vec<Value> {
    request
        .messages
        .iter()
        .map(|message| {
            let role = match message.role {
                crate::MessageRole::System => "system",
                crate::MessageRole::User => "user",
                crate::MessageRole::Assistant => "assistant",
                crate::MessageRole::Tool => "tool",
            };
            let mut value = json!({"role": role, "content": message.content});
            if let Some(call_id) = &message.tool_call_id {
                value["tool_call_id"] = json!(call_id);
            }
            if !message.tool_calls.is_empty() {
                value["tool_calls"] = json!(message
                    .tool_calls
                    .iter()
                    .map(|call| {
                        if openai {
                            json!({
                                "id": call.id,
                                "type": "function",
                                "function": {
                                    "name": call.name,
                                    "arguments": call.arguments_json,
                                }
                            })
                        } else {
                            let arguments = serde_json::from_str::<Value>(&call.arguments_json)
                                .unwrap_or_else(|_| Value::String(call.arguments_json.clone()));
                            json!({
                                "id": call.id,
                                "function": {
                                    "name": call.name,
                                    "arguments": arguments,
                                }
                            })
                        }
                    })
                    .collect::<Vec<_>>());
            }
            value
        })
        .collect()
}

fn tools_json(request: &ProviderRequest) -> Vec<Value> {
    request
        .tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters_json_schema,
                }
            })
        })
        .collect()
}

#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Deserialize)]
struct OpenAiMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAiToolCall>>,
}

#[derive(Deserialize)]
struct OpenAiToolCall {
    id: Option<String>,
    function: OpenAiFunction,
}

#[derive(Deserialize)]
struct OpenAiFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
}

#[derive(Deserialize)]
struct OllamaMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OllamaToolCall>>,
}

#[derive(Deserialize)]
struct OllamaToolCall {
    id: Option<String>,
    function: OllamaFunction,
}

#[derive(Deserialize)]
struct OllamaFunction {
    name: String,
    arguments: Value,
}
