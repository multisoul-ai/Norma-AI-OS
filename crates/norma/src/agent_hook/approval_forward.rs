use super::{
    parse_hook_event, AgentHookError, HookApprovalRequest, HookForwardEnv, HookSource,
    APPROVAL_TIMEOUT,
};
use serde_json::Value;
use std::io::{self, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::Duration;

/// Only the per-session permission Hook calls this. Existing observation Hooks stay unchanged.
pub fn forward_permission(
    source: HookSource,
    raw: Value,
    env: HookForwardEnv,
    run_id: String,
) -> Result<Option<Value>, AgentHookError> {
    if source != HookSource::Claude || run_id.is_empty() {
        return Err(invalid("unsupported permission bridge"));
    }
    let event = parse_hook_event(source, env.node_id, raw)?;
    if event.provider_event != "PermissionRequest" {
        return Err(invalid("not a permission request"));
    }
    let url = url::Url::parse(&env.endpoint).map_err(|_| invalid("invalid hook endpoint"))?;
    let ip: IpAddr = url
        .host_str()
        .and_then(|host| host.parse().ok())
        .filter(IpAddr::is_loopback)
        .ok_or_else(|| invalid("permission bridge requires loopback"))?;
    if url.scheme() != "http"
        || url.path() != "/agent-hooks"
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || env.token.contains(['\r', '\n'])
    {
        return Err(invalid("invalid permission endpoint or token"));
    }
    let address = SocketAddr::new(ip, url.port().ok_or_else(|| invalid("missing port"))?);
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
    // Server waits 120 seconds; two seconds cover the final local transport response.
    stream.set_read_timeout(Some(APPROVAL_TIMEOUT + Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let body = serde_json::to_vec(&HookApprovalRequest { run_id, event })?;
    write!(stream, "POST /agent-hooks/approval HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", env.token, body.len())?;
    stream.write_all(&body)?;
    stream.flush()?;
    // Permission outputs are tiny; bound even a compromised/malformed local response.
    let mut response = Vec::new();
    stream.take(8193).read_to_end(&mut response)?;
    if response.len() > 8192 {
        return Err(invalid("permission response too large"));
    }
    let split = response
        .windows(4)
        .position(|chunk| chunk == b"\r\n\r\n")
        .ok_or_else(|| invalid("incomplete permission response"))?;
    let head =
        std::str::from_utf8(&response[..split]).map_err(|_| invalid("invalid response headers"))?;
    let mut lines = head.split("\r\n");
    let status = lines.next().unwrap_or_default();
    let mut length = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return Err(invalid("invalid header"));
        };
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
        return Err(invalid("incomplete permission body"));
    }
    match status.split_whitespace().nth(1) {
        Some("204") if body.is_empty() => Ok(None), // Fallback to native confirmation.
        Some("200") => {
            let output: Value = serde_json::from_slice(body)?;
            let specific = &output["hookSpecificOutput"];
            if specific["hookEventName"] != "PermissionRequest"
                || !matches!(
                    specific["decision"]["behavior"].as_str(),
                    Some("allow" | "deny")
                )
            {
                return Err(invalid("invalid permission decision"));
            }
            // Reconstruct only this request's decision; never accept updated permissions/input.
            let decision = if specific["decision"]["behavior"] == "allow" {
                super::ApprovalDecision::Allow
            } else {
                super::ApprovalDecision::Deny
            };
            Ok(Some(super::claude_permission_output(decision)))
        }
        _ => Err(invalid("permission request rejected")),
    }
}

fn invalid(message: &str) -> AgentHookError {
    AgentHookError::Io(io::Error::new(io::ErrorKind::InvalidData, message))
}
