use super::{ConversationPage, ConversationRequest};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ConversationEvent {
    pub stream_id: String,
    pub sequence: u64,
    pub request: ConversationRequest,
    /// None is an authorized, freshly read heartbeat; it does not advance the sequence.
    pub page: Option<ConversationPage>,
}

pub struct ConversationProjection {
    stream_id: String,
    sequence: u64,
    request: ConversationRequest,
    page: Option<ConversationPage>,
}
impl ConversationProjection {
    pub fn new(stream_id: String, request: ConversationRequest) -> Self {
        Self {
            stream_id,
            sequence: 0,
            request,
            page: None,
        }
    }
    pub fn update(&mut self, page: ConversationPage) -> Result<ConversationEvent, &'static str> {
        let change = if self.page.as_ref() == Some(&page) {
            None
        } else {
            if self.page.is_some() {
                self.sequence = self.sequence.checked_add(1).ok_or("stream_exhausted")?;
            }
            self.page = Some(page.clone());
            Some(page)
        };
        Ok(ConversationEvent {
            stream_id: self.stream_id.clone(),
            sequence: self.sequence,
            request: self.request.clone(),
            page: change,
        })
    }
}
