#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Speaker {
    User,
    Assistant,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptFragment {
    pub event_id: String,
    pub speaker: Speaker,
    pub delta: String,
    pub start_ms: f64,
    pub end_ms: f64,
}
