use serde::{Deserialize, Serialize};

// 64KiB × worst-case JSON escape factor 6 = 384KiB, leaving >100KiB envelope space.
pub const CONVERSATION_CHUNK_BYTES: usize = 64 * 1024;
// Claude scans at most16MiB; nested tool JSON escaping can nearly double the response.
pub const MAX_CONVERSATION_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ConversationFragment {
    pub transfer_id: String,
    pub offset: usize,
    pub total_bytes: usize,
    pub text: String,
}
impl ConversationFragment {
    pub fn at(transfer_id: &str, payload: &str, offset: usize) -> Result<Self, &'static str> {
        if payload.len() > MAX_CONVERSATION_BYTES {
            return Err("history_too_large");
        }
        if offset >= payload.len() || !payload.is_char_boundary(offset) {
            return Err("invalid_fragment");
        }
        let mut end = (offset + CONVERSATION_CHUNK_BYTES).min(payload.len());
        while !payload.is_char_boundary(end) {
            end -= 1;
        }
        Ok(Self {
            transfer_id: transfer_id.into(),
            offset,
            total_bytes: payload.len(),
            text: payload[offset..end].into(),
        })
    }
}
