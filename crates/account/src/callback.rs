//! 解析回环回调请求目标（`/callback?code=…&state=…`）。纯函数，无 IO。

use crate::pkce::verify_state;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallbackError {
    Malformed,
    MissingState,
    StateMismatch,
    ProviderDenied(String),
    MissingCode,
}

/// 先校验 `state`，再看提供方 `error`，最后取 `code`，避免回显未经校验的错误。
pub fn parse_callback(target: &str, expected_state: &str) -> Result<String, CallbackError> {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if path != "/callback" {
        return Err(CallbackError::Malformed);
    }
    let mut code = None;
    let mut state = None;
    let mut error = None;
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let slot = match key {
            "code" => &mut code,
            "state" => &mut state,
            "error" => &mut error,
            _ => continue,
        };
        if slot.is_none() {
            *slot = Some(percent_decode(value)?);
        }
    }
    let state = state.ok_or(CallbackError::MissingState)?;
    if !verify_state(expected_state, &state) {
        return Err(CallbackError::StateMismatch);
    }
    if let Some(error) = error {
        return Err(CallbackError::ProviderDenied(error));
    }
    code.filter(|c| !c.is_empty())
        .ok_or(CallbackError::MissingCode)
}

fn percent_decode(raw: &str) -> Result<String, CallbackError> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3).ok_or(CallbackError::Malformed)?;
            let hex = std::str::from_utf8(hex).map_err(|_| CallbackError::Malformed)?;
            out.push(u8::from_str_radix(hex, 16).map_err(|_| CallbackError::Malformed)?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| CallbackError::Malformed)
}
