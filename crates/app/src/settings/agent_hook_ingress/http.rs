use norma::agent_hook::{AgentHookEvent, HookApprovalRequest, HookQuestionRequest};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;

pub(super) enum HookRequest {
    Event(AgentHookEvent),
    Approval(HookApprovalRequest),
    Question(HookQuestionRequest),
}

pub(super) fn read_hook_request(
    stream: &TcpStream,
) -> Result<(String, HookRequest), AgentHookHttpError> {
    // 16 KiB headers + 1 MiB event: prevent untrusted Content-Length allocations.
    let mut reader = BufReader::new(stream.take(16_384 + 1_048_576 + 1));
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let kind = match request_line.trim_end() {
        "POST /agent-hooks HTTP/1.1" => "event",
        "POST /agent-hooks/approval HTTP/1.1" => "approval",
        "POST /agent-hooks/question HTTP/1.1" => "question",
        _ => return Err(AgentHookHttpError::BadRequest),
    };
    let mut token = None;
    let mut content_len = None;
    let mut header_bytes = request_line.len();
    loop {
        let mut line = String::new();
        let size = reader.read_line(&mut line)?;
        header_bytes += size;
        if size == 0 || header_bytes > 16_384 {
            return Err(AgentHookHttpError::BadRequest);
        }
        if line == "\r\n" {
            break;
        }
        let (key, value) = line
            .trim_end_matches(['\r', '\n'])
            .split_once(':')
            .ok_or(AgentHookHttpError::BadRequest)?;
        if key.eq_ignore_ascii_case("authorization") {
            if token.is_some() {
                return Err(AgentHookHttpError::Unauthorized);
            }
            token = Some(
                value
                    .trim()
                    .strip_prefix("Bearer ")
                    .ok_or(AgentHookHttpError::Unauthorized)?
                    .to_owned(),
            );
        }
        if key.eq_ignore_ascii_case("transfer-encoding") {
            return Err(AgentHookHttpError::BadRequest);
        }
        if key.eq_ignore_ascii_case("content-length") {
            if content_len.is_some() {
                return Err(AgentHookHttpError::BadRequest);
            }
            content_len = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| AgentHookHttpError::BadRequest)?,
            );
        }
    }
    let token = token
        .filter(|value| !value.is_empty())
        .ok_or(AgentHookHttpError::Unauthorized)?;
    let content_len = content_len
        .filter(|length| *length > 0 && *length <= 1_048_576)
        .ok_or(AgentHookHttpError::BadRequest)?;
    let mut body = vec![0_u8; content_len];
    reader.read_exact(&mut body)?;
    let request = if kind == "question" {
        HookRequest::Question(serde_json::from_slice(&body).map_err(|_| AgentHookHttpError::Json)?)
    } else if kind == "approval" {
        HookRequest::Approval(serde_json::from_slice(&body).map_err(|_| AgentHookHttpError::Json)?)
    } else {
        let mut event: AgentHookEvent =
            serde_json::from_slice(&body).map_err(|_| AgentHookHttpError::Json)?;
        // Older installed bridges omit the envelope turn ID but retain the original Provider JSON.
        // Recover only a missing field, never replace a supplied identity.
        if event.turn_id.is_none() {
            event.turn_id = ["turn_id", "turnId", "prompt_id"].iter().find_map(|key| {
                event.raw_json[*key]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .map(str::to_owned)
            });
        }
        HookRequest::Event(event)
    };
    Ok((token, request))
}

pub(super) fn write_status(stream: &mut TcpStream, status: &str) {
    let _ = stream.write_all(&response(status, &[]));
    let _ = stream.flush();
}

pub(super) fn response(status: &str, body: &[u8]) -> Vec<u8> {
    let mut response = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
    response.extend_from_slice(body);
    response
}

#[derive(Debug)]
pub(crate) enum AgentHookHttpError {
    Io,
    Json,
    Unauthorized,
    BadRequest,
}
impl From<std::io::Error> for AgentHookHttpError {
    fn from(_: std::io::Error) -> Self {
        Self::Io
    }
}
