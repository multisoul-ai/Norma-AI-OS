use crate::protocol::{
    build_ollama_chat_request, build_openai_chat_request, parse_ollama_response,
    parse_openai_response, ProviderConfig,
};
use crate::{
    AgentProvider, ProviderError, ProviderRequest, ProviderResponse, ProviderStreamEvent,
    ProviderToolCall,
};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
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
        let raw = send_http_request(&self.config, request, self.timeout)?;
        parse_ollama_stream(&raw, emit)
    }
}

pub struct OpenAiCompatibleProvider {
    config: ProviderConfig,
    timeout: Duration,
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
        let raw = send_http_request(&self.config, request, self.timeout)?;
        parse_openai_stream(&raw, emit)
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
    let endpoint = config
        .endpoint()
        .strip_prefix("http://")
        .ok_or_else(|| ProviderError::Message("provider endpoint must use http://".to_string()))?;
    let host_port = endpoint.split('/').next().unwrap_or_default();
    let address = host_port
        .to_socket_addrs()
        .map_err(|_| ProviderError::Message("provider endpoint could not resolve".to_string()))?
        .next()
        .ok_or_else(|| ProviderError::Message("provider endpoint has no address".to_string()))?;
    let mut stream = TcpStream::connect_timeout(&address, timeout)
        .map_err(|_| ProviderError::Message("provider connection failed".to_string()))?;
    stream
        .set_read_timeout(Some(timeout))
        .and_then(|_| stream.set_write_timeout(Some(timeout)))
        .map_err(|_| ProviderError::Message("provider socket setup failed".to_string()))?;
    stream
        .write_all(request.as_bytes())
        .map_err(|_| ProviderError::Message("provider request failed".to_string()))?;
    let mut raw = String::new();
    stream
        .read_to_string(&mut raw)
        .map_err(|_| ProviderError::Message("provider response failed".to_string()))?;
    decode_http_body(&raw)
}

fn decode_http_body(raw: &str) -> Result<String, ProviderError> {
    let (status, rest) = raw
        .split_once("\r\n")
        .ok_or_else(|| ProviderError::Message("provider response missing status".to_string()))?;
    if !status.contains(" 200 ") {
        return Err(ProviderError::Message(
            "provider returned a non-200 response".to_string(),
        ));
    }
    let (_, body) = rest
        .split_once("\r\n\r\n")
        .ok_or_else(|| ProviderError::Message("provider response missing body".to_string()))?;
    if !raw
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        return Ok(body.to_string());
    }
    decode_chunked_body(body)
}

fn decode_chunked_body(mut body: &str) -> Result<String, ProviderError> {
    let mut decoded = String::new();
    loop {
        let (size, rest) = body
            .split_once("\r\n")
            .ok_or_else(|| ProviderError::Message("provider chunked body truncated".to_string()))?;
        let size = usize::from_str_radix(size.trim(), 16)
            .map_err(|_| ProviderError::Message("provider chunk size invalid".to_string()))?;
        if size == 0 {
            return Ok(decoded);
        }
        if rest.len() < size + 2 {
            return Err(ProviderError::Message(
                "provider chunk payload truncated".to_string(),
            ));
        }
        decoded.push_str(&rest[..size]);
        body = &rest[size + 2..];
    }
}

fn parse_openai_stream(
    raw: &str,
    emit: &mut dyn FnMut(ProviderStreamEvent),
) -> Result<ProviderResponse, ProviderError> {
    let mut text = String::new();
    let mut calls: Vec<ProviderToolCall> = Vec::new();
    for line in raw.lines().filter_map(|line| line.strip_prefix("data: ")) {
        if line == "[DONE]" {
            continue;
        }
        let chunk: OpenAiStreamChunk = serde_json::from_str(line).map_err(|error| {
            ProviderError::Message(format!("OpenAI stream JSON failed: {error}"))
        })?;
        let Some(choice) = chunk.choices.into_iter().next() else {
            continue;
        };
        if let Some(delta) = choice.delta.content {
            text.push_str(&delta);
            emit(ProviderStreamEvent::TextDelta(delta));
        }
        for call in choice.delta.tool_calls.unwrap_or_default() {
            if let Some(existing) = calls.get_mut(call.index) {
                if let Some(arguments) = call.function.arguments {
                    existing.arguments_json.push_str(&arguments);
                }
                if let Some(name) = call.function.name {
                    existing.name = name;
                }
                if let Some(id) = call.id {
                    existing.id = id;
                }
            } else {
                let normalized = ProviderToolCall {
                    id: call
                        .id
                        .unwrap_or_else(|| format!("openai-stream-call-{}", call.index)),
                    name: call.function.name.unwrap_or_default(),
                    arguments_json: call.function.arguments.unwrap_or_default(),
                };
                calls.resize_with(call.index + 1, || ProviderToolCall {
                    id: String::new(),
                    name: String::new(),
                    arguments_json: String::new(),
                });
                calls[call.index] = normalized;
            }
        }
    }
    for call in &calls {
        emit(ProviderStreamEvent::ToolCall(call.clone()));
    }
    Ok(ProviderResponse {
        text,
        tool_calls: calls,
    })
}

fn parse_ollama_stream(
    raw: &str,
    emit: &mut dyn FnMut(ProviderStreamEvent),
) -> Result<ProviderResponse, ProviderError> {
    let mut text = String::new();
    let mut calls = Vec::new();
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        let response = crate::parse_ollama_response(line)?;
        if !response.text.is_empty() {
            text.push_str(&response.text);
            emit(ProviderStreamEvent::TextDelta(response.text));
        }
        calls.extend(response.tool_calls);
    }
    for call in &calls {
        emit(ProviderStreamEvent::ToolCall(call.clone()));
    }
    Ok(ProviderResponse {
        text,
        tool_calls: calls,
    })
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
