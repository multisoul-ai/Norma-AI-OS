//! A bounded transfer on its own authenticated socket; no partial files survive cancellation.
use super::{conversation::send, failure, success, Action, Request, Shared};
use base64::{engine::general_purpose::STANDARD, Engine};
use norma::mobile::{ConversationFragment, MobileSocket};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

#[path = "image_store.rs"]
mod store;
const LIMIT: usize = 2 * 1024 * 1024;
const CHUNK: usize = 64 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Chunk {
    offset: usize,
    data: String,
}

pub(super) fn serve(
    socket: &mut MobileSocket,
    shared: &Arc<Shared>,
    socket_id: u64,
    request: Request,
) {
    if let Err(code) = run(socket, shared, socket_id, &request) {
        let _ = send(socket, failure(&request.request_id, code));
    }
}

fn run(
    socket: &mut MobileSocket,
    shared: &Shared,
    socket_id: u64,
    request: &Request,
) -> Result<(), &'static str> {
    let (device, token, workspace, image_id, upload) = match &request.action {
        Action::UploadImage {
            device_id,
            token,
            workspace_id,
            image_id,
            byte_count,
            grant_revision,
        } => (
            device_id,
            token,
            workspace_id,
            image_id,
            Some((*byte_count, *grant_revision)),
        ),
        Action::ChatImage {
            device_id,
            token,
            workspace_id,
            image_id,
        } => (device_id, token, workspace_id, image_id, None),
        _ => return Err("invalid_request"),
    };
    if !crate::mobile::command::valid_id(workspace)
        || image_id.len() != 64
        || !image_id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || upload.is_some_and(|(size, _)| size == 0 || size > LIMIT)
    {
        return Err("invalid_image");
    }
    let authorize = || -> Result<PathBuf, &'static str> {
        let access = shared.access().map_err(|_| "unavailable")?;
        let grant = access
            .authenticate(&request.machine_id, device, token)
            .map_err(|_| "unauthorized")?
            .ok_or("unauthorized")?;
        if !grant.allows(workspace, upload.is_some()) {
            return Err("unauthorized");
        }
        if upload.is_some_and(|(_, revision)| revision != grant.revision) {
            return Err("grant_changed");
        }
        Ok(access
            .store
            .path()
            .with_extension("mobile-images")
            .join(workspace))
    };
    let folder = authorize()?;
    shared
        .sockets
        .lock()
        .map_err(|_| "unavailable")?
        .get_mut(&socket_id)
        .ok_or("unavailable")?
        .device_id = Some(device.clone());
    // 60s total includes up to32 chunks, not a renewable per-chunk timeout.
    let deadline = Instant::now() + Duration::from_secs(60);
    if let Some((size, _)) = upload {
        send(
            socket,
            success(
                &request.request_id,
                json!({"status":"image_upload","image_offset":0}),
            ),
        )?;
        let mut bytes = Vec::with_capacity(size);
        while bytes.len() < size {
            authorize()?;
            if Instant::now() >= deadline {
                return Err("image_timeout");
            }
            socket.get_mut().sock.set_deadline(deadline);
            let frame = socket.read().map_err(|_| "unavailable")?;
            let text = frame.to_text().map_err(|_| "invalid_image")?;
            let chunk: Chunk = serde_json::from_str(text).map_err(|_| "invalid_image")?;
            if chunk.offset != bytes.len() || chunk.data.len() > CHUNK.div_ceil(3) * 4 {
                return Err("invalid_image");
            }
            let data = STANDARD.decode(&chunk.data).map_err(|_| "invalid_image")?;
            if data.is_empty() || data.len() > CHUNK || data.len() > size - bytes.len() {
                return Err("invalid_image");
            }
            bytes.extend(data);
            if bytes.len() < size {
                send(
                    socket,
                    success(
                        &request.request_id,
                        json!({"status":"image_upload","image_offset":bytes.len()}),
                    ),
                )?;
            }
        }
        if format!("{:x}", Sha256::digest(&bytes)) != *image_id {
            return Err("invalid_image");
        }
        let extension = store::validate(&bytes)?;
        authorize()?;
        let path = store::save(&folder, image_id, extension, &bytes)?;
        // Revocation during decode/write cannot report a usable upload receipt.
        authorize()?;
        return send(
            socket,
            success(
                &request.request_id,
                json!({"status":"image_saved",
            "image":{"id":image_id,"path":path,"byte_count":bytes.len()}}),
            ),
        );
    }
    let (path, bytes) = store::read(&folder, image_id)?;
    let payload =
        json!({"id":image_id,"path":path,"byte_count":bytes.len(),"data":STANDARD.encode(bytes)});
    let text = payload.to_string();
    if text.len() < 128 * 1024 {
        authorize()?;
        return send(
            socket,
            success(
                &request.request_id,
                json!({"status":"chat_image","image":payload}),
            ),
        );
    }
    let mut offset = 0;
    while offset < text.len() {
        authorize()?;
        if Instant::now() >= deadline {
            return Err("image_timeout");
        }
        let fragment = ConversationFragment::at(image_id, &text, offset)?;
        offset += fragment.text.len();
        send(
            socket,
            success(
                &request.request_id,
                json!({"status":"image_fragment","fragment":fragment}),
            ),
        )?;
    }
    Ok(())
}
