//! The dedicated question bridge can only answer its original AskUserQuestion invocation.
use super::{
    parse_hook_event, AgentHookError, ClaudeQuestion, HookForwardEnv, HookQuestionRequest,
    HookSource, QUESTION_TIMEOUT,
};
use serde_json::{json, Value};
use std::io::{self, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::Duration;

pub fn forward_question(
    source: HookSource,
    raw: Value,
    env: HookForwardEnv,
    run_id: String,
) -> Result<Option<Value>, AgentHookError> {
    if source != HookSource::Claude || run_id.is_empty() || run_id.len() > 128 {
        return Err(invalid("unsupported question bridge"));
    }
    let shape = ClaudeQuestion::from_hook(&raw);
    let original = raw["tool_input"].clone();
    let mut event = parse_hook_event(source, env.node_id, raw)?;
    event.control_run_id = Some(run_id.clone());
    exchange_question(&env, &HookQuestionRequest { run_id, event }, shape.err())?
        .map(|output| validate_output(original, output))
        .transpose()
}

/// Background-only request from a native transport. Return data, never an executable RPC.
pub fn forward_native_question(
    request: HookQuestionRequest,
    env: &HookForwardEnv,
) -> Result<Option<super::NativeQuestionReply>, AgentHookError> {
    if request.event.node_id != env.node_id
        || request.event.control_run_id.as_deref() != Some(request.run_id.as_str())
        || request.event.profile != crate::protocol::AgentProfile::Codex
    {
        return Err(invalid("native question scope mismatch"));
    }
    let ticket = super::HookQuestion::new(request.clone(), std::time::Instant::now())?;
    let Some(output) = exchange_question(env, &request, None)? else {
        return Ok(None);
    };
    // Reconstruct only known data fields; server-supplied RPC/configuration fields are ignored.
    let reply = super::NativeQuestionReply {
        delivery_id: output["delivery_id"]
            .as_str()
            .ok_or_else(|| invalid("missing delivery id"))?
            .into(),
        answers: serde_json::from_value(output["answers"].clone())?,
    };
    reply.validate(ticket.adapter()).map_err(invalid)?;
    Ok(Some(reply))
}

fn exchange_question(
    env: &HookForwardEnv,
    request: &HookQuestionRequest,
    unsupported: Option<&str>,
) -> Result<Option<Value>, AgentHookError> {
    let url = url::Url::parse(&env.endpoint).map_err(|_| invalid("invalid hook endpoint"))?;
    let ip: IpAddr = url
        .host_str()
        .and_then(|host| host.parse().ok())
        .filter(IpAddr::is_loopback)
        .ok_or_else(|| invalid("question bridge requires loopback"))?;
    if url.scheme() != "http"
        || url.path() != "/agent-hooks"
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || env.token.is_empty()
        || env.token.contains(['\r', '\n'])
    {
        return Err(invalid("invalid question endpoint or token"));
    }
    let address = SocketAddr::new(ip, url.port().ok_or_else(|| invalid("missing port"))?);
    let body = serde_json::to_vec(request)?;
    if body.len() > 1_048_576 {
        return Err(invalid("question request too large"));
    }
    if let Some(reason) = unsupported {
        // Unsupported mapping still leaves a live native question. Preserve its input guard.
        super::forward_event(&env.endpoint, &env.token, &request.event)?;
        return Err(invalid(reason));
    }
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(QUESTION_TIMEOUT + Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    write!(stream, "POST /agent-hooks/question HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", env.token, body.len())?;
    stream.write_all(&body)?;
    stream.flush()?;
    let mut response = Vec::new();
    stream.take(1_064_961).read_to_end(&mut response)?;
    if response.len() > 1_064_960 {
        return Err(invalid("question response too large"));
    }
    let split = response
        .windows(4)
        .position(|chunk| chunk == b"\r\n\r\n")
        .filter(|split| *split <= 16_384)
        .ok_or_else(|| invalid("invalid response headers"))?;
    let head =
        std::str::from_utf8(&response[..split]).map_err(|_| invalid("invalid response headers"))?;
    let mut lines = head.split("\r\n");
    let status = lines.next().unwrap_or_default();
    let mut length = None;
    for line in lines {
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| invalid("invalid header"))?;
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(invalid("unsupported transfer encoding"));
        }
        if name.eq_ignore_ascii_case("content-length") {
            if length.is_some() {
                return Err(invalid("duplicate content length"));
            }
            length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| invalid("invalid content length"))?,
            );
        }
    }
    let body = &response[split + 4..];
    if length != Some(body.len()) {
        return Err(invalid("incomplete question body"));
    }
    match status.split_whitespace().nth(1) {
        Some("204") if body.is_empty() => Ok(None),
        Some("200") => Ok(Some(serde_json::from_slice(body)?)),
        _ => Err(invalid("question request rejected")),
    }
}

fn validate_output(mut original: Value, output: Value) -> Result<Value, AgentHookError> {
    let specific = &output["hookSpecificOutput"];
    if specific["hookEventName"] != "PreToolUse" || specific["permissionDecision"] != "allow" {
        return Err(invalid("invalid question response"));
    }
    let mut updated = specific["updatedInput"].clone();
    let answers = updated
        .as_object_mut()
        .and_then(|input| input.remove("answers"))
        .ok_or_else(|| invalid("missing question answers"))?;
    let mapped = answers
        .as_object()
        .ok_or_else(|| invalid("invalid question answers"))?;
    original
        .as_object_mut()
        .ok_or_else(|| invalid("invalid original input"))?
        .remove("answers");
    if original != updated {
        return Err(invalid("question input changed"));
    }
    let questions = original["questions"]
        .as_array()
        .ok_or_else(|| invalid("invalid original questions"))?;
    if mapped.len() != questions.len()
        || !questions.iter().all(|question| {
            question["question"]
                .as_str()
                .and_then(|prompt| mapped.get(prompt))
                .and_then(Value::as_str)
                .is_some_and(|answer| !answer.trim().is_empty() && answer.len() <= 32768)
        })
    {
        return Err(invalid("incomplete question answers"));
    }
    // Only the original input and answers cross stdout; ignore server permission/config fields.
    original["answers"] = answers;
    Ok(
        json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow","updatedInput":original}}),
    )
}

fn invalid(message: &str) -> AgentHookError {
    AgentHookError::Io(io::Error::new(io::ErrorKind::InvalidData, message))
}
