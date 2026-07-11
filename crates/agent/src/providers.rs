use crate::protocol::{
    build_ollama_chat_request, build_openai_chat_request, parse_ollama_response,
    parse_openai_response, ProviderConfig,
};
use crate::{
    AgentProvider, ProviderError, ProviderRequest, ProviderResponse, ProviderStreamEvent,
    ProviderToolCall,
};
use std::io::{BufRead, BufReader};
use std::time::Duration;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

pub struct OllamaProvider {
    config: ProviderConfig,
    timeout: Duration,
}

impl OllamaProvider {
    pub fn new(config: ProviderConfig) -> Self {
        Self {
            config,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn config(&self) -> &ProviderConfig {
        &self.config
    }
}

impl AgentProvider for OllamaProvider {
    fn complete(&mut self, request: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        let request = build_ollama_chat_request(&self.config, request)?;
        let raw = send_http_request(&self.config, request, self.timeout)?;
        parse_ollama_response(&raw)
    }

    fn stream(
        &mut self,
        request: &ProviderRequest,
        emit: &mut dyn FnMut(ProviderStreamEvent),
    ) -> Result<ProviderResponse, ProviderError> {
        let request = build_ollama_chat_request(&self.config, request)?
            .replace("\"stream\":false", "\"stream\":true");
        send_streaming_request(&self.config, request, self.timeout, emit)
    }
}

pub struct OpenAiCompatibleProvider {
    config: ProviderConfig,
    timeout: Duration,
}

pub enum ConfiguredProvider {
    Ollama(OllamaProvider),
    OpenAiCompatible(OpenAiCompatibleProvider),
}

impl ConfiguredProvider {
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        config.validate()?;
        Ok(match config.protocol() {
            crate::ProviderProtocol::Ollama => Self::Ollama(OllamaProvider::new(config)),
            crate::ProviderProtocol::OpenAiCompatible => {
                Self::OpenAiCompatible(OpenAiCompatibleProvider::new(config))
            }
        })
    }

    pub fn protocol(&self) -> crate::ProviderProtocol {
        match self {
            Self::Ollama(_) => crate::ProviderProtocol::Ollama,
            Self::OpenAiCompatible(_) => crate::ProviderProtocol::OpenAiCompatible,
        }
    }
}

impl AgentProvider for ConfiguredProvider {
    fn complete(&mut self, request: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        match self {
            Self::Ollama(provider) => provider.complete(request),
            Self::OpenAiCompatible(provider) => provider.complete(request),
        }
    }

    fn stream(
        &mut self,
        request: &ProviderRequest,
        emit: &mut dyn FnMut(ProviderStreamEvent),
    ) -> Result<ProviderResponse, ProviderError> {
        match self {
            Self::Ollama(provider) => provider.stream(request, emit),
            Self::OpenAiCompatible(provider) => provider.stream(request, emit),
        }
    }
}

impl OpenAiCompatibleProvider {
    pub fn new(config: ProviderConfig) -> Self {
        Self {
            config,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn config(&self) -> &ProviderConfig {
        &self.config
    }
}

impl AgentProvider for OpenAiCompatibleProvider {
    fn complete(&mut self, request: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        let request = build_openai_chat_request(&self.config, request)?;
        let raw = send_http_request(&self.config, request, self.timeout)?;
        parse_openai_response(&raw)
    }

    fn stream(
        &mut self,
        request: &ProviderRequest,
        emit: &mut dyn FnMut(ProviderStreamEvent),
    ) -> Result<ProviderResponse, ProviderError> {
        let request = build_openai_stream_request(&self.config, request)?;
        send_streaming_request(&self.config, request, self.timeout, emit)
    }
}

fn build_openai_stream_request(
    config: &ProviderConfig,
    request: &ProviderRequest,
) -> Result<String, ProviderError> {
    let mut wire = build_openai_chat_request(config, request)?;
    wire = wire.replace("\"stream\":false", "\"stream\":true");
    Ok(wire)
}

fn send_http_request(
    config: &ProviderConfig,
    request: String,
    timeout: Duration,
) -> Result<String, ProviderError> {
    let (url, body) = request_parts(config, &request)?;
    tracing::debug!(
        protocol = ?config.protocol(),
        model = config.model(),
        request_body_bytes = body.len(),
        "[DEBUG-agent-flow-7c1e] provider non-streaming request started"
    );
    let mut response = send_request(config, &url, body, timeout)?;
    let raw = response
        .body_mut()
        .read_to_string()
        .map_err(|_| ProviderError::Message("provider response failed".to_string()))?;
    tracing::debug!(
        protocol = ?config.protocol(),
        model = config.model(),
        response_body_bytes = raw.len(),
        "[DEBUG-agent-flow-7c1e] provider non-streaming response received"
    );
    Ok(raw)
}

fn send_streaming_request(
    config: &ProviderConfig,
    request: String,
    timeout: Duration,
    emit: &mut dyn FnMut(ProviderStreamEvent),
) -> Result<ProviderResponse, ProviderError> {
    let (url, body) = request_parts(config, &request)?;
    tracing::debug!(
        protocol = ?config.protocol(),
        model = config.model(),
        request_body_bytes = body.len(),
        "[DEBUG-agent-flow-7c1e] provider streaming request started"
    );
    let response = send_request(config, &url, body, timeout)?;
    let reader = BufReader::new(response.into_body().into_reader());
    let mut text = String::new();
    let mut calls = Vec::new();
    let mut line_count = 0_usize;
    let mut openai_data_line_count = 0_usize;
    let mut accepted_openai_data_line_count = 0_usize;
    for line in reader.lines() {
        let line =
            line.map_err(|_| ProviderError::Message("provider response failed".to_string()))?;
        line_count += 1;
        if line.starts_with("data:") {
            openai_data_line_count += 1;
        }
        if line.starts_with("data: ") {
            accepted_openai_data_line_count += 1;
        }
        match config.protocol() {
            crate::ProviderProtocol::OpenAiCompatible => {
                consume_openai_stream_line(&line, &mut text, &mut calls, emit)?;
            }
            crate::ProviderProtocol::Ollama => {
                consume_ollama_stream_line(&line, &mut text, &mut calls, emit)?;
            }
        }
    }
    if config.protocol() == crate::ProviderProtocol::OpenAiCompatible {
        for call in &calls {
            emit(ProviderStreamEvent::ToolCall(call.clone()));
        }
        if openai_data_line_count > accepted_openai_data_line_count {
            tracing::warn!(
                protocol = ?config.protocol(),
                model = config.model(),
                openai_data_line_count,
                accepted_openai_data_line_count,
                "[DEBUG-agent-flow-7c1e] OpenAI stream contained data lines rejected by the strict prefix parser"
            );
        }
    }
    let tool_names = calls
        .iter()
        .map(|call: &ProviderToolCall| call.name.as_str())
        .collect::<Vec<_>>()
        .join(",");
    tracing::debug!(
        protocol = ?config.protocol(),
        model = config.model(),
        line_count,
        openai_data_line_count,
        accepted_openai_data_line_count,
        text_chars = text.chars().count(),
        tool_call_count = calls.len(),
        tool_names,
        "[DEBUG-agent-flow-7c1e] provider streaming response consumed"
    );
    Ok(ProviderResponse {
        text,
        tool_calls: calls,
    })
}

fn send_request(
    config: &ProviderConfig,
    url: &str,
    body: &str,
    timeout: Duration,
) -> Result<ureq::http::Response<ureq::Body>, ProviderError> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .new_agent();
    let mut request = agent.post(url).header("Content-Type", "application/json");
    if let Some(api_key) = config.api_key().filter(|key| !key.trim().is_empty()) {
        request = request.header("Authorization", &format!("Bearer {api_key}"));
    }
    let response = request
        .send(body)
        .map_err(|error| provider_transport_error(error, config.api_key()));
    match &response {
        Ok(_) => tracing::debug!(
            protocol = ?config.protocol(),
            model = config.model(),
            "[DEBUG-agent-flow-7c1e] provider HTTP request completed"
        ),
        Err(error) => tracing::warn!(
            protocol = ?config.protocol(),
            model = config.model(),
            %error,
            "[DEBUG-agent-flow-7c1e] provider HTTP request failed"
        ),
    }
    response
}

fn request_parts<'a>(
    config: &ProviderConfig,
    request: &'a str,
) -> Result<(String, &'a str), ProviderError> {
    config.validate()?;
    let request_line = request.lines().next().ok_or_else(|| {
        ProviderError::Message("provider request missing request line".to_string())
    })?;
    let path = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| ProviderError::Message("provider request missing path".to_string()))?;
    let scheme_end = config.endpoint().find("://").ok_or_else(|| {
        ProviderError::Message("provider base_url must use http:// or https://".to_string())
    })? + 3;
    let authority_end = config.endpoint()[scheme_end..]
        .find('/')
        .map(|index| scheme_end + index)
        .unwrap_or(config.endpoint().len());
    let origin = &config.endpoint()[..authority_end];
    let (_, body) = request
        .split_once("\r\n\r\n")
        .ok_or_else(|| ProviderError::Message("provider request missing body".to_string()))?;
    Ok((format!("{origin}{path}"), body))
}

fn provider_transport_error(error: ureq::Error, api_key: Option<&str>) -> ProviderError {
    let mut message = error.to_string();
    if let Some(api_key) = api_key.filter(|key| !key.is_empty()) {
        message = message.replace(api_key, "<redacted>");
    }
    ProviderError::Message(format!("provider request failed: {message}"))
}

pub fn parse_openai_stream(
    raw: &str,
    emit: &mut dyn FnMut(ProviderStreamEvent),
) -> Result<ProviderResponse, ProviderError> {
    let mut text = String::new();
    let mut calls: Vec<ProviderToolCall> = Vec::new();
    for line in raw.lines() {
        consume_openai_stream_line(line, &mut text, &mut calls, emit)?;
    }
    for call in &calls {
        emit(ProviderStreamEvent::ToolCall(call.clone()));
    }
    Ok(ProviderResponse {
        text,
        tool_calls: calls,
    })
}

fn consume_openai_stream_line(
    line: &str,
    text: &mut String,
    calls: &mut Vec<ProviderToolCall>,
    emit: &mut dyn FnMut(ProviderStreamEvent),
) -> Result<(), ProviderError> {
    let Some(line) = line.strip_prefix("data: ") else {
        return Ok(());
    };
    if line == "[DONE]" {
        return Ok(());
    }
    let chunk: OpenAiStreamChunk = serde_json::from_str(line)
        .map_err(|error| ProviderError::Message(format!("OpenAI stream JSON failed: {error}")))?;
    let Some(choice) = chunk.choices.into_iter().next() else {
        return Ok(());
    };
    if let Some(delta) = choice.delta.content {
        text.push_str(&delta);
        emit(ProviderStreamEvent::TextDelta(delta));
    }
    for call in choice.delta.tool_calls.unwrap_or_default() {
        if calls.len() <= call.index {
            calls.resize_with(call.index + 1, || ProviderToolCall {
                id: String::new(),
                name: String::new(),
                arguments_json: String::new(),
            });
        }
        let existing = &mut calls[call.index];
        if existing.id.is_empty() {
            existing.id = format!("openai-stream-call-{}", call.index);
        }
        if let Some(arguments) = call.function.arguments {
            existing.arguments_json.push_str(&arguments);
        }
        if let Some(name) = call.function.name {
            existing.name = name;
        }
        if let Some(id) = call.id {
            existing.id = id;
        }
    }
    Ok(())
}

pub fn parse_ollama_stream(
    raw: &str,
    emit: &mut dyn FnMut(ProviderStreamEvent),
) -> Result<ProviderResponse, ProviderError> {
    let mut text = String::new();
    let mut calls = Vec::new();
    for line in raw.lines() {
        consume_ollama_stream_line(line, &mut text, &mut calls, emit)?;
    }
    Ok(ProviderResponse {
        text,
        tool_calls: calls,
    })
}

fn consume_ollama_stream_line(
    line: &str,
    text: &mut String,
    calls: &mut Vec<ProviderToolCall>,
    emit: &mut dyn FnMut(ProviderStreamEvent),
) -> Result<(), ProviderError> {
    if line.trim().is_empty() {
        return Ok(());
    }
    let response = crate::parse_ollama_response(line)?;
    if !response.text.is_empty() {
        text.push_str(&response.text);
        emit(ProviderStreamEvent::TextDelta(response.text));
    }
    for call in response.tool_calls {
        emit(ProviderStreamEvent::ToolCall(call.clone()));
        calls.push(call);
    }
    Ok(())
}

#[derive(serde::Deserialize)]
struct OpenAiStreamChunk {
    choices: Vec<OpenAiStreamChoice>,
}

#[derive(serde::Deserialize)]
struct OpenAiStreamChoice {
    delta: OpenAiStreamDelta,
}

#[derive(serde::Deserialize)]
struct OpenAiStreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAiStreamToolCall>>,
}

#[derive(serde::Deserialize)]
struct OpenAiStreamToolCall {
    index: usize,
    id: Option<String>,
    function: OpenAiStreamFunction,
}

#[derive(serde::Deserialize)]
struct OpenAiStreamFunction {
    name: Option<String>,
    arguments: Option<String>,
}
