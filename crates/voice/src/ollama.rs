use crate::ollama_repair::repair_plan_with_required_hint;
use crate::{CardRef, VoiceCardKind, VoicePlan, VoicePlanError, VoicePlanningRequest};
use norma::protocol::AgentProfile;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

const DEFAULT_OLLAMA_MODEL: &str = "qwen3.5:0.8b";

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

pub(crate) fn voice_trace_enabled() -> bool {
    voice_trace_enabled_from_value(std::env::var("MULTISOUL_VOICE_TRACE").ok())
}

pub(crate) fn voice_trace_enabled_from_value(value: Option<String>) -> bool {
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
    think: bool,
    format: &'static str,
    options: OllamaGenerateOptions,
}

#[derive(Serialize)]
struct OllamaGenerateOptions {
    temperature: f32,
}

#[derive(Deserialize)]
pub(crate) struct OllamaGenerateResponse {
    pub(crate) response: String,
}

fn planner_prompt(request: &VoicePlanningRequest) -> Result<String, serde_json::Error> {
    let transcript = serde_json::to_string(&request.transcript)?;
    let targets = serde_json::to_string(&planner_targets(request))?;
    Ok(format!(
        "You are Multisoul's local voice planner.\n\n\
         Return exactly one JSON object. No markdown. No text outside JSON.\n\n\
         CURRENT_TRANSCRIPT:\n{}\n\n\
         TARGETS:\n{}\n\n\
         OUTPUT_CONTRACT:\n{}\n\n\
         RULES:\n{}",
        transcript,
        targets,
        output_contract_prompt(),
        planning_rules_prompt()
    ))
}

#[derive(Serialize)]
struct PlannerTargets<'a> {
    focused: Option<u64>,
    selected: &'a [u64],
    cards: Vec<PlannerTargetCard<'a>>,
}

#[derive(Serialize)]
struct PlannerTargetCard<'a> {
    node_id: u64,
    name: &'a str,
    kind: String,
    selected: bool,
    focused: bool,
}

fn planner_targets(request: &VoicePlanningRequest) -> PlannerTargets<'_> {
    PlannerTargets {
        focused: request.focused_node_id,
        selected: &request.selected_node_ids,
        cards: request
            .cards
            .iter()
            .map(|card| PlannerTargetCard {
                node_id: card.node_id,
                name: planner_target_name(card),
                kind: planner_target_kind(&card.kind),
                selected: card.selected,
                focused: card.focused,
            })
            .collect(),
    }
}

fn planner_target_name(card: &CardRef) -> &str {
    card.alias
        .as_deref()
        .filter(|alias| !alias.trim().is_empty())
        .unwrap_or(&card.title)
}

fn planner_target_kind(kind: &VoiceCardKind) -> String {
    match kind {
        VoiceCardKind::Terminal => "terminal".to_string(),
        VoiceCardKind::Agent { profile } => format!("agent:{}", agent_profile_name(*profile)),
        VoiceCardKind::Preview => "preview".to_string(),
        VoiceCardKind::Rect => "rect".to_string(),
    }
}

fn agent_profile_name(profile: AgentProfile) -> &'static str {
    match profile {
        AgentProfile::Claude => "Claude",
        AgentProfile::Codex => "Codex",
    }
}

fn output_contract_prompt() -> &'static str {
    r#"Return JSON like {"commands":[],"confidence":0.9,"risk":"low","requires_confirmation":false,"reason":"short reason"}.
Allowed commands:
{"type":"create","kind":{"kind":"terminal"}}
{"type":"create","kind":{"kind":"agent","profile":"Codex"}}
{"type":"create","kind":{"kind":"agent","profile":"Claude"}}
{"type":"focus","target":{"type":"node_id","node_id":1}}
{"type":"send_text","target":{"type":"previous_step","step":0},"text":"hello","submit":false}
Allowed targets: node_id, alias, previous_step, focused, selected. Allowed risk: low, medium, high."#
}

fn planning_rules_prompt() -> &'static str {
    "1. Every explicit requested card/entity in CURRENT_TRANSCRIPT must become a create or focus command.\n\
     2. Do not merge multiple requested cards into one command. Create one command for every requested card.\n\
     3. If a count is requested, repeat create commands for that count.\n\
     4. 打开codex, 开启codex, 创建codex, or open/create Codex means create a new agent profile Codex card.\n\
     5. Claude Code, 打开claude code, 开启claude code, or open/create Claude means create a new agent profile Claude card.\n\
     6. Terminal, terminal, 终端, 命令行 mean create terminal.\n\
     7. 输入, 发送, 发给, send, input, type mean send_text.\n\
     8. If send_text happens after newly created cards and no explicit target is named, target the most recently created compatible card with previous_step.\n\
     9. Use TARGETS only for existing-card routing by node_id, alias/name, focused, or selected.\n\
     10. 不要提交 or 不执行 means submit=false. 提交, 执行, or 回车 means submit=true.\n\
     11. target and kind must be single objects, never arrays.\n\
     12. Do not output delete, list, unsupported commands, or natural-language-only plans.\n\
     13. If uncertain, set confidence below 0.65."
}

fn build_generate_body(model: &str, prompt: &str) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&OllamaGenerateRequest {
        model,
        prompt,
        stream: false,
        think: false,
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

pub(crate) fn post_ollama_generate(
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
    let address = host_port
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| std::io::Error::other("ollama endpoint resolved no addresses"))?;
    let mut stream = TcpStream::connect_timeout(&address, timeout)?;
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

#[cfg(test)]
#[path = "../tests/unit/ollama_prompt_tests.rs"]
mod prompt_tests;
