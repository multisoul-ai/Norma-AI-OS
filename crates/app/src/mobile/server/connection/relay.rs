use super::{decode, failure, legacy, success, workspace_credentials, Action, Shared};
use anyhow::{ensure, Result};
use serde_json::json;
use std::sync::Arc;

/// Pairing and old project streams remain local-only; new workspace RPCs share the LAN handler.
pub(in crate::mobile::server) fn handle(
    text: &str,
    shared: &Arc<Shared>,
    authenticated: &mut Option<String>,
) -> Result<Vec<u8>> {
    let request = match decode(text) {
        Ok(request) => request,
        Err(reply) => return Ok(serde_json::to_vec(&reply)?),
    };
    if let Some((device, token)) = workspace_credentials(&request.action) {
        ensure!(
            authenticated.as_deref() == Some(device),
            "relay authentication required"
        );
        let reply = match crate::mobile::workspace_chat::handle(
            shared,
            &request.machine_id,
            device,
            token,
            &request.action,
        ) {
            Ok(result) => success(&request.request_id, result),
            Err(code) => failure(&request.request_id, code),
        };
        return Ok(serde_json::to_vec(&reply)?);
    }
    let (device, token) = match &request.action {
        Action::Authenticate { device_id, token }
        | Action::Snapshot {
            device_id, token, ..
        } => (device_id.as_str(), token.as_str()),
        action => legacy::credentials(action)
            .ok_or_else(|| anyhow::anyhow!("unsupported relay action"))?,
    };
    ensure!(
        authenticated.as_deref().is_none_or(|id| id == device),
        "relay device changed"
    );
    ensure!(
        authenticated.is_some() || matches!(request.action, Action::Authenticate { .. }),
        "relay authentication required"
    );
    let grant = shared
        .access()?
        .authenticate(&request.machine_id, device, token)?
        .ok_or_else(|| anyhow::anyhow!("unauthorized relay device"))?;
    let result = match &request.action {
        Action::Authenticate { .. } => {
            *authenticated = Some(device.into());
            let mut capabilities = vec!["legacy_thread_history_v1"];
            if shared.workspaces.is_some() {
                capabilities.extend([
                    "workspace_chat_v1",
                    "workspace_agent_create_v1",
                    "workspace_terminal_v1",
                ]);
            }
            Ok(json!({"status":"authorized", "grant":grant, "capabilities":capabilities}))
        }
        Action::Snapshot { workspace_id, .. } => crate::mobile::snapshot::read(
            shared,
            &request.machine_id,
            device,
            token,
            workspace_id.as_deref(),
        ),
        _ => legacy::read(shared, &request, device, token),
    };
    let reply = match result {
        Ok(result) => success(&request.request_id, result),
        Err("unauthorized") => anyhow::bail!("relay permission changed"),
        Err(code) => failure(&request.request_id, code),
    };
    Ok(serde_json::to_vec(&reply)?)
}
