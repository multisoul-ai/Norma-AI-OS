use super::{
    parse_hook_event, AgentHookError, AgentHookEvent, HookForwardEnv, HookForwardOutcome,
    HookSource,
};
use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

pub fn forward_hook(
    source: HookSource,
    raw_json: Value,
    env: Option<HookForwardEnv>,
) -> HookForwardOutcome {
    let Some(env) = env else {
        return HookForwardOutcome::Noop;
    };
    let Ok(event) = parse_hook_event(source, env.node_id, raw_json) else {
        return HookForwardOutcome::Noop;
    };
    match forward_event(&env.endpoint, &env.token, &event) {
        Ok(()) => HookForwardOutcome::Posted,
        Err(error) => {
            eprintln!("multisoul-agent-hook: ignored forward failure: {error}");
            HookForwardOutcome::FailedButIgnored
        }
    }
}

pub fn forward_event(
    endpoint: &str,
    token: &str,
    event: &AgentHookEvent,
) -> Result<(), AgentHookError> {
    let (host, port, path) = parse_http_endpoint(endpoint)?;
    let body = serde_json::to_vec(event)?;
    let mut stream = TcpStream::connect((host.as_str(), port))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)?;
    stream.flush()?;
    let mut response = [0u8; 32];
    let read = stream.read(&mut response)?;
    let status_line = std::str::from_utf8(&response[..read]).unwrap_or("");
    if status_line.starts_with("HTTP/1.1 2") || status_line.starts_with("HTTP/1.0 2") {
        Ok(())
    } else {
        Err(AgentHookError::UnsupportedEndpoint(status_line.to_string()))
    }
}

fn parse_http_endpoint(endpoint: &str) -> Result<(String, u16, String), AgentHookError> {
    let rest = endpoint
        .strip_prefix("http://")
        .ok_or_else(|| AgentHookError::UnsupportedEndpoint(endpoint.to_string()))?;
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (host, port) = authority
        .rsplit_once(':')
        .ok_or_else(|| AgentHookError::UnsupportedEndpoint(endpoint.to_string()))?;
    let port = port
        .parse::<u16>()
        .map_err(|_| AgentHookError::UnsupportedEndpoint(endpoint.to_string()))?;
    Ok((host.to_string(), port, format!("/{path}")))
}
