use crate::ollama_repair::{repair_plan_with_required_hint, required_plan_hint_json};
use crate::{VoicePlan, VoicePlanError, VoicePlanningRequest};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const DEFAULT_OLLAMA_MODEL: &str = "qwen2.5:1.5b";

pub trait VoicePlanner {
    fn plan(&self, request: &VoicePlanningRequest) -> Result<VoicePlan, VoicePlanProviderError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OllamaVoicePlanner {
    endpoint: String,
    model: String,
    timeout: Duration,
}

#[derive(Debug, thiserror::Error)]
pub enum VoicePlanProviderError {
    #[error("voice plan validation failed: {0}")]
    Plan(#[from] VoicePlanError),
    #[error("ollama endpoint must be http://host:port")]
    InvalidEndpoint,
    #[error("ollama request failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("ollama json failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ollama response was missing a JSON body")]
    MissingBody,
}

impl OllamaVoicePlanner {
    pub fn new(endpoint: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            timeout: Duration::from_secs(8),
        }
    }

    pub fn from_env() -> Option<Self> {
        let model = std::env::var("MULTISOUL_OLLAMA_MODEL").ok()?;
        let endpoint = std::env::var("MULTISOUL_OLLAMA_ENDPOINT")
            .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string());
        Some(Self::new(endpoint, model))
    }

    pub fn from_env_or_default() -> Self {
        let model = Self::configured_model();
        let endpoint = std::env::var("MULTISOUL_OLLAMA_ENDPOINT")
            .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string());
        Self::new(endpoint, model)
    }

    pub fn default_model() -> &'static str {
        DEFAULT_OLLAMA_MODEL
    }

    pub fn configured_model() -> String {
        std::env::var("MULTISOUL_OLLAMA_MODEL").unwrap_or_else(|_| DEFAULT_OLLAMA_MODEL.to_string())
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

impl VoicePlanner for OllamaVoicePlanner {
    fn plan(&self, request: &VoicePlanningRequest) -> Result<VoicePlan, VoicePlanProviderError> {
        let prompt = planner_prompt(request)?;
        let trace_full = voice_trace_enabled();
        log_voice_planner_request(self, request, &prompt, trace_full);
        let response = post_ollama_generate(&self.endpoint, &self.model, &prompt, self.timeout)?;
        log_voice_planner_response(self, &response.response, trace_full);
        let parsed_plan = parse_voice_plan_response(&response.response)?;
        let (plan, repaired) = repair_plan_with_required_hint(request, parsed_plan);
        if repaired {
            tracing::warn!(
                target: "voice::ollama",
                model = %self.model,
                transcript = %request.transcript,
                "voice planner output repaired from deterministic required hint"
            );
        }
        tracing::info!(
            target: "voice::ollama",
            model = %self.model,
            commands = plan.commands.len(),
            confidence = plan.confidence,
            risk = ?plan.risk,
            requires_confirmation = plan.requires_confirmation,
            reason = plan.reason.as_deref().unwrap_or(""),
            "voice planner parsed plan"
        );
        Ok(plan)
    }
}

fn log_voice_planner_request(
    planner: &OllamaVoicePlanner,
    request: &VoicePlanningRequest,
    prompt: &str,
    trace_full: bool,
) {
    let request_json =
        serde_json::to_string(request).unwrap_or_else(|_| "<unserializable request>".to_string());
    tracing::info!(
        target: "voice::ollama",
        endpoint = %planner.endpoint,
        model = %planner.model,
        request = %request_json,
        prompt_chars = prompt.chars().count(),
        "voice planner request"
    );
    if trace_full {
        tracing::info!(
            target: "voice::ollama",
            endpoint = %planner.endpoint,
            model = %planner.model,
            prompt = %prompt,
            "voice planner prompt"
        );
    }
}

fn log_voice_planner_response(planner: &OllamaVoicePlanner, response: &str, trace_full: bool) {
    tracing::info!(
        target: "voice::ollama",
        endpoint = %planner.endpoint,
        model = %planner.model,
        response = %response,
        response_chars = response.chars().count(),
        "voice planner raw response"
    );
    if trace_full {
        tracing::info!(
            target: "voice::ollama",
            endpoint = %planner.endpoint,
            model = %planner.model,
            response_debug = ?response,
            "voice planner raw response debug"
        );
    }
}

fn voice_trace_enabled() -> bool {
    voice_trace_enabled_from_value(std::env::var("MULTISOUL_VOICE_TRACE").ok())
}

fn voice_trace_enabled_from_value(value: Option<String>) -> bool {
    value
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
}

#[derive(Serialize)]
struct OllamaGenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
    format: &'static str,
    options: OllamaGenerateOptions,
}

#[derive(Serialize)]
struct OllamaGenerateOptions {
    temperature: f32,
}

#[derive(Deserialize)]
struct OllamaGenerateResponse {
    response: String,
}

fn planner_prompt(request: &VoicePlanningRequest) -> Result<String, serde_json::Error> {
    let context = serde_json::to_string(request)?;
    let schema = serde_json::to_string(&planner_schema(request.max_steps))?;
    let required_hint = serde_json::to_string(&required_plan_hint_json(request))?;
    let transcript = serde_json::to_string(&request.transcript)?;
    Ok(format!(
        "You are Multisoul's local voice planner.\n\n\
         Return exactly one JSON object. No markdown. No text outside JSON.\n\n\
         CURRENT_TRANSCRIPT:\n{}\n\n\
         ALLOWED_SCHEMA:\n{}\n\n\
         CURRENT_CANVAS_CONTEXT:\n{}\n\n\
         REQUIRED_OUTPUT_HINT:\n{}\n\n\
         TASK:\nConvert CURRENT_TRANSCRIPT into a VoicePlan JSON object.\n\n\
         OUTPUT_SHAPE:\n{}\n\n\
         PLANNING_RULES:\n{}\n\n\
         SELF_CHECK_BEFORE_OUTPUT:\n{}\n\n\
         EXAMPLES:\n{}",
        transcript,
        schema,
        context,
        required_hint,
        output_shape_prompt(),
        planning_rules_prompt(request.max_steps),
        self_check_prompt(),
        examples_prompt()
    ))
}

fn planner_schema(max_steps: usize) -> serde_json::Value {
    serde_json::json!({
        "command_shapes": {
            "create": {
                "type": "create",
                "kind": "one of allowed_card_kinds"
            },
            "focus": {
                "type": "focus",
                "target": "one of allowed_targets"
            },
            "send_text": {
                "type": "send_text",
                "target": "one of allowed_targets",
                "text": "string",
                "submit": false
            }
        },
        "allowed_card_kinds": {
            "terminal": { "kind": "terminal" },
            "agent_claude": { "kind": "agent", "profile": "Claude" },
            "agent_codex": { "kind": "agent", "profile": "Codex" }
        },
        "allowed_targets": {
            "node_id": { "type": "node_id", "node_id": 1 },
            "alias": { "type": "alias", "alias": "card alias" },
            "previous_step": { "type": "previous_step", "step": 0 },
            "focused": { "type": "focused" },
            "selected": { "type": "selected" }
        },
        "risk_values": {
            "low": "low",
            "medium": "medium",
            "high": "high"
        },
        "max_steps": max_steps,
        "forbidden_commands": {
            "delete": "never output",
            "list": "never output"
        }
    })
}

fn output_shape_prompt() -> &'static str {
    r#"{"commands":[],"confidence":0.9,"risk":"low","requires_confirmation":false,"reason":"string"}"#
}

fn planning_rules_prompt(max_steps: usize) -> String {
    format!(
        "1. Every explicit requested card/entity in CURRENT_TRANSCRIPT must become a create or focus command.\n\
         2. Do not merge multiple requested cards into one command. Create one command for every requested card.\n\
         3. If a count is requested, repeat create commands for that count.\n\
         4. 打开codex, 开启codex, 创建codex, or open/create Codex means create a new agent profile Codex card even if Codex cards already exist.\n\
         5. Claude Code, 打开claude code, 开启claude code, or open/create Claude means create a new agent profile Claude card even if Claude cards already exist.\n\
         6. Terminal, terminal, 终端, 命令行 mean create terminal.\n\
         7. 输入, 发送, 发给, send, input, type mean send_text.\n\
         8. If send_text happens after newly created cards and no explicit target is named, target the most recently created compatible card with previous_step.\n\
         9. previous_step must refer to a prior create command index; do not always use previous_step 0.\n\
         10. 不要提交 or 不执行 means submit=false. 提交, 执行, or 回车 means submit=true.\n\
         11. target must be one object, never an array of target names.\n\
         12. kind must be one object, never an array of card kind names.\n\
         13. If REQUIRED_OUTPUT_HINT is not null, follow its commands exactly unless CURRENT_CANVAS_CONTEXT contradicts it.\n\
         14. Do not output delete, list, unsupported commands, or natural-language-only plans.\n\
         15. Use at most {max_steps} commands.\n\
         16. If you cannot satisfy every explicit requested entity, set confidence below 0.65."
    )
}

fn self_check_prompt() -> &'static str {
    "Internally list all requested cards/entities from CURRENT_TRANSCRIPT. \
     Verify each requested card/entity has a matching command. Verify send_text target points \
     to the intended card. Then output only the final JSON."
}

fn examples_prompt() -> &'static str {
    r#"Input: "打开codex,claude code, 再开启两个终端 然后输入hi"
Output: {"commands":[{"type":"create","kind":{"kind":"agent","profile":"Codex"}},{"type":"create","kind":{"kind":"agent","profile":"Claude"}},{"type":"create","kind":{"kind":"terminal"}},{"type":"create","kind":{"kind":"terminal"}},{"type":"send_text","target":{"type":"previous_step","step":3},"text":"hi","submit":false}],"confidence":0.9,"risk":"low","requires_confirmation":false,"reason":"create Codex, Claude, two terminals, then type hi in the last terminal"}

Input: "创建一个 Codex 卡片，并向它发送 hello，不要提交"
Output: {"commands":[{"type":"create","kind":{"kind":"agent","profile":"Codex"}},{"type":"send_text","target":{"type":"previous_step","step":0},"text":"hello","submit":false}],"confidence":0.9,"risk":"low","requires_confirmation":false,"reason":"create Codex and send hello without submitting"}"#
}

fn build_generate_body(model: &str, prompt: &str) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&OllamaGenerateRequest {
        model,
        prompt,
        stream: false,
        format: "json",
        options: OllamaGenerateOptions { temperature: 0.0 },
    })
}

fn parse_voice_plan_response(response: &str) -> Result<VoicePlan, VoicePlanProviderError> {
    let json = extract_json_object(response).ok_or(VoicePlanProviderError::MissingBody)?;
    Ok(serde_json::from_str(json)?)
}

fn extract_json_object(response: &str) -> Option<&str> {
    let start = response.find('{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (offset, ch) in response[start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    let end = start + offset + ch.len_utf8();
                    return Some(&response[start..end]);
                }
            }
            _ => {}
        }
    }

    None
}

fn post_ollama_generate(
    endpoint: &str,
    model: &str,
    prompt: &str,
    timeout: Duration,
) -> Result<OllamaGenerateResponse, VoicePlanProviderError> {
    let stripped = endpoint
        .strip_prefix("http://")
        .ok_or(VoicePlanProviderError::InvalidEndpoint)?;
    let mut parts = stripped.splitn(2, '/');
    let host_port = parts
        .next()
        .filter(|value| !value.is_empty())
        .ok_or(VoicePlanProviderError::InvalidEndpoint)?;
    let path_prefix = parts.next().unwrap_or("").trim_end_matches('/');
    let path = if path_prefix.is_empty() {
        "/api/generate".to_string()
    } else {
        format!("/{path_prefix}/api/generate")
    };

    let body = build_generate_body(model, prompt)?;
    let mut stream = TcpStream::connect(host_port)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: {host_port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)?;

    let mut raw = String::new();
    stream.read_to_string(&mut raw)?;
    parse_ollama_http_response(&raw)
}

fn parse_ollama_http_response(raw: &str) -> Result<OllamaGenerateResponse, VoicePlanProviderError> {
    let (headers, body) = raw
        .split_once("\r\n\r\n")
        .ok_or(VoicePlanProviderError::MissingBody)?;
    let decoded = if headers
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        decode_chunked_body(body)?
    } else {
        body.to_string()
    };
    Ok(serde_json::from_str(&decoded)?)
}

fn decode_chunked_body(body: &str) -> Result<String, VoicePlanProviderError> {
    let mut rest = body;
    let mut decoded = String::new();

    loop {
        let (size_line, after_size) = rest
            .split_once("\r\n")
            .ok_or(VoicePlanProviderError::MissingBody)?;
        let size_hex = size_line.split(';').next().unwrap_or(size_line);
        let size = usize::from_str_radix(size_hex.trim(), 16)
            .map_err(|_| VoicePlanProviderError::MissingBody)?;
        if size == 0 {
            return Ok(decoded);
        }
        if after_size.len() < size + 2 {
            return Err(VoicePlanProviderError::MissingBody);
        }

        let (chunk, after_chunk) = after_size.split_at(size);
        decoded.push_str(chunk);
        rest = after_chunk
            .strip_prefix("\r\n")
            .ok_or(VoicePlanProviderError::MissingBody)?;
    }
}

#[cfg(test)]
#[path = "../tests/unit/ollama_tests.rs"]
mod tests;
