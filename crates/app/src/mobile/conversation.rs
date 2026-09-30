use super::server::Shared;
use norma::mobile::{ConversationPage, ConversationRequest};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub struct LiveConversationRequest {
    pub request: ConversationRequest,
    pub archive: Option<storage::MobileSessionKey>,
    pub deadline: Instant,
    pub reply: crossbeam_channel::Sender<Result<ConversationPage, &'static str>>,
}

pub(super) fn read(
    shared: &Shared,
    machine: &str,
    device: &str,
    token: &str,
    request: ConversationRequest,
    archive: Option<storage::MobileSessionKey>,
) -> Result<Value, &'static str> {
    if !request.valid()
        || archive.as_ref().is_some_and(|key| {
            key.validate().is_err()
                || key.node_id != request.node_id
                || key.run_id != request.run_id
                || key.session_id != request.session_id
        })
    {
        return Err("invalid_request");
    }
    let before = {
        let access = shared.access().map_err(|_| "unavailable")?;
        let grant = access
            .authenticate(machine, device, token)
            .map_err(|_| "unauthorized")?
            .ok_or("unauthorized")?;
        if !grant.allows(&request.workspace_id, false) {
            return Err("unauthorized");
        }
        grant.revision
    };
    let source = shared.conversations.as_ref().ok_or("unavailable")?;
    let (reply, result) = crossbeam_channel::bounded(1);
    // 4s leaves 1s of the iOS exchange budget for wire delivery. Read-only timeouts may be retried.
    // Archive cold startup shares a12s total budget; native first response allows15s.
    let deadline = Instant::now() + Duration::from_secs(if archive.is_some() { 12 } else { 4 });
    source
        .try_send(LiveConversationRequest {
            request: request.clone(),
            archive,
            deadline,
            reply,
        })
        .map_err(|_| "unavailable")?;
    let page = result
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| "unavailable")??;
    let access = shared.access().map_err(|_| "unavailable")?;
    let current = access
        .authenticate(machine, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    if current.revision != before || !current.allows(&request.workspace_id, false) {
        return Err("unauthorized");
    }
    let result = json!({"status":"conversation","conversation":{"request":request,"page":page}});
    // Large pages use bounded transport fragments; never truncate message or tool records.
    if result.to_string().len() > norma::mobile::MAX_CONVERSATION_BYTES {
        return Err("history_too_large");
    }
    Ok(result)
}
