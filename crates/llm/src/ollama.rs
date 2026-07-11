use crate::{
    ChatMessage, ChatRequest, ChatResponse, LlmClient, LlmError, Role, ToolCall, ToolSpec,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OllamaLlmClient {
    endpoint: String,
    model: String,
    timeout: Duration,
}

impl OllamaLlmClient {
    pub fn new(endpoint: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn model(&self) -> &str {
        &self.model
    }
}

impl LlmClient for OllamaLlmClient {
    fn chat(&mut self, request: &ChatRequest) -> Result<ChatResponse, LlmError> {
        let body = build_chat_body(&self.model, request)?;
        let raw = post_http(
            &self.endpoint,
            "POST",
            "/api/chat",
            Some(&body),
            self.timeout,
        )?;
        parse_chat_response(&raw)
    }
}

pub fn probe_ollama_ready(endpoint: &str) -> Result<(), LlmError> {
    let _ = fetch_ollama_models(endpoint)?;
    Ok(())
}

pub fn probe_ollama_model_ready(endpoint: &str, model: &str) -> Result<(), LlmError> {
    let models = fetch_ollama_models(endpoint)?;
    if models
        .iter()
        .any(|installed| model_names_match(installed, model))
    {
        return Ok(());
    }
    Err(LlmError::Message(format!(
        "Ollama model '{model}' is not installed; run: ollama pull {model}"
    )))
}

fn fetch_ollama_models(endpoint: &str) -> Result<Vec<String>, LlmError> {
    let raw = get_http(endpoint, "/api/tags", PROBE_TIMEOUT)?;
    let value: Value = serde_json::from_str(&raw)
        .map_err(|err| LlmError::Message(format!("ollama tags json failed: {err}")))?;
    let models = value
        .get("models")
        .and_then(Value::as_array)
        .ok_or_else(|| LlmError::Message("ollama tags response missing models".into()))?;
    Ok(models
        .iter()
        .filter_map(|entry| {
            entry
                .get("name")
                .or_else(|| entry.get("model"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect())
}

fn model_names_match(installed: &str, configured: &str) -> bool {
    installed == configured
        || installed
            .strip_suffix(":latest")
            .is_some_and(|name| name == configured)
        || configured
            .strip_suffix(":latest")
            .is_some_and(|name| name == installed)
}

fn build_chat_body(model: &str, request: &ChatRequest) -> Result<Vec<u8>, LlmError> {
    let messages: Vec<Value> = request
        .messages
        .iter()
        .map(chat_message_to_json)
        .collect::<Result<_, _>>()?;
    let tools: Vec<Value> = request.tools.iter().map(tool_spec_to_json).collect();
    let payload = json!({
        "model": model,
        "messages": messages,
        "tools": tools,
        "think": false,
        "options": {
            "num_predict": 128,
        },
        "stream": false,
    });
    serde_json::to_vec(&payload)
        .map_err(|err| LlmError::Message(format!("ollama chat request json failed: {err}")))
}

fn chat_message_to_json(message: &ChatMessage) -> Result<Value, LlmError> {
    let role = match message.role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    };
    let mut value = json!({
        "role": role,
        "content": message.content,
    });
    if let Some(tool_call_id) = message.tool_call_id.as_deref() {
        value["tool_call_id"] = json!(tool_call_id);
    }
    if !message.tool_calls.is_empty() {
        let tool_calls: Vec<Value> = message
            .tool_calls
            .iter()
            .map(assistant_tool_call_to_json)
            .collect();
        value["tool_calls"] = json!(tool_calls);
    }
    Ok(value)
}

fn assistant_tool_call_to_json(call: &ToolCall) -> Value {
    json!({
        "id": call.id,
        "type": "function",
        "function": {
            "name": call.name,
            "arguments": call.arguments_json,
        }
    })
}

fn tool_spec_to_json(spec: &ToolSpec) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": spec.name,
            "description": spec.description,
            "parameters": spec.parameters_json_schema,
        }
    })
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    message: OllamaChatMessage,
}

#[derive(Deserialize)]
struct OllamaChatMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OllamaToolCall>>,
}

#[derive(Deserialize)]
struct OllamaToolCall {
    id: Option<String>,
    function: OllamaToolFunction,
}

#[derive(Deserialize)]
struct OllamaToolFunction {
    name: String,
    arguments: Value,
}

fn parse_chat_response(raw: &str) -> Result<ChatResponse, LlmError> {
    let response: OllamaChatResponse = serde_json::from_str(raw)
        .map_err(|err| LlmError::Message(format!("ollama chat json failed: {err}")))?;
    let assistant_text = response.message.content.unwrap_or_default();
    let tool_calls = response
        .message
        .tool_calls
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(index, call)| parse_tool_call(index, call))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ChatResponse {
        assistant_text,
        tool_calls,
    })
}

fn parse_tool_call(index: usize, call: OllamaToolCall) -> Result<ToolCall, LlmError> {
    let arguments_json = match call.function.arguments {
        Value::String(text) => text,
        other => serde_json::to_string(&other).map_err(|err| {
            LlmError::Message(format!("ollama tool arguments json failed: {err}"))
        })?,
    };
    Ok(ToolCall {
        id: call
            .id
            .unwrap_or_else(|| format!("ollama-tool-call-{index}")),
        name: call.function.name,
        arguments_json,
    })
}

fn get_http(endpoint: &str, path: &str, timeout: Duration) -> Result<String, LlmError> {
    post_http(endpoint, "GET", path, None, timeout)
}

fn post_http(
    endpoint: &str,
    method: &str,
    path: &str,
    body: Option<&[u8]>,
    timeout: Duration,
) -> Result<String, LlmError> {
    let (host_port, request_path) = parse_endpoint(endpoint, path)?;
    let mut stream = connect_endpoint(&host_port, timeout)?;
    if let Some(body) = body {
        write!(
            stream,
            "{method} {request_path} HTTP/1.1\r\nHost: {host_port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .map_err(io_err)?;
        stream.write_all(body).map_err(io_err)?;
    } else {
        write!(
            stream,
            "{method} {request_path} HTTP/1.1\r\nHost: {host_port}\r\nConnection: close\r\n\r\n"
        )
        .map_err(io_err)?;
    }

    let mut raw = String::new();
    stream.read_to_string(&mut raw).map_err(io_err)?;
    decode_http_json_body(&raw)
}

fn parse_endpoint(endpoint: &str, path: &str) -> Result<(String, String), LlmError> {
    let stripped = endpoint
        .strip_prefix("http://")
        .ok_or_else(|| LlmError::Message("ollama endpoint must be http://host:port".into()))?;
    let mut parts = stripped.splitn(2, '/');
    let host_port = parts
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| LlmError::Message("ollama endpoint must be http://host:port".into()))?
        .to_string();
    let path_prefix = parts.next().unwrap_or("").trim_end_matches('/');
    let request_path = if path_prefix.is_empty() {
        path.to_string()
    } else {
        format!("/{path_prefix}{path}")
    };
    Ok((host_port, request_path))
}

fn connect_endpoint(host_port: &str, timeout: Duration) -> Result<TcpStream, LlmError> {
    let address = host_port
        .to_socket_addrs()
        .map_err(io_err)?
        .next()
        .ok_or_else(|| LlmError::Message("ollama endpoint resolved no addresses".into()))?;
    let stream = TcpStream::connect_timeout(&address, timeout).map_err(io_err)?;
    stream.set_read_timeout(Some(timeout)).map_err(io_err)?;
    stream.set_write_timeout(Some(timeout)).map_err(io_err)?;
    Ok(stream)
}

fn decode_http_json_body(raw: &str) -> Result<String, LlmError> {
    let (status_line, rest) = raw
        .split_once("\r\n")
        .ok_or_else(|| LlmError::Message("ollama response missing status line".into()))?;
    if !status_line.contains(" 200 ") {
        return Err(LlmError::Message(format!(
            "ollama http error: {status_line}"
        )));
    }
    let (headers, body) = rest
        .split_once("\r\n\r\n")
        .ok_or_else(|| LlmError::Message("ollama response missing body".into()))?;
    let decoded = if headers
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        decode_chunked_body(body)?
    } else {
        body.to_string()
    };
    Ok(decoded)
}

fn decode_chunked_body(body: &str) -> Result<String, LlmError> {
    let mut rest = body;
    let mut decoded = String::new();

    loop {
        let (size_line, after_size) = rest
            .split_once("\r\n")
            .ok_or_else(|| LlmError::Message("ollama chunked body truncated".into()))?;
        let size_hex = size_line.split(';').next().unwrap_or(size_line);
        let size = usize::from_str_radix(size_hex.trim(), 16)
            .map_err(|_| LlmError::Message("ollama chunked body size invalid".into()))?;
        if size == 0 {
            return Ok(decoded);
        }
        if after_size.len() < size + 2 {
            return Err(LlmError::Message(
                "ollama chunked body payload truncated".into(),
            ));
        }

        let (chunk, after_chunk) = after_size.split_at(size);
        decoded.push_str(chunk);
        rest = after_chunk
            .strip_prefix("\r\n")
            .ok_or_else(|| LlmError::Message("ollama chunked body terminator missing".into()))?;
    }
}

fn io_err(error: std::io::Error) -> LlmError {
    LlmError::Message(format!("ollama request failed: {error}"))
}

#[cfg(test)]
#[path = "../tests/unit/ollama_tests.rs"]
mod tests;
