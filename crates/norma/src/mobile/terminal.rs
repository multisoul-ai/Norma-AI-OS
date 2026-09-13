use super::CardSnapshot;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TerminalRequest {
    pub workspace_id: String,
    pub route_id: String,
    pub node_id: u64,
    pub run_id: String,
    pub session_id: Option<String>,
    pub before: Option<usize>,
    pub revision: Option<u64>,
}
impl TerminalRequest {
    pub fn valid(&self) -> bool {
        self.node_id > 0
            && [&self.workspace_id, &self.route_id, &self.run_id]
                .into_iter()
                .chain(self.session_id.iter())
                .all(|s| {
                    !s.is_empty()
                        && s.len() <= 128
                        && s.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                })
            && (self.before.is_some() == self.revision.is_some())
            && self.before.is_none_or(|n| n > 0 && n <= 100_000)
    }
}

/// The desktop's original terminal grid; each page copies at most120×512 cells.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TerminalPage {
    pub card: CardSnapshot,
    pub revision: u64,
    pub screen_signature: u64,
    pub input_sequence: u64,
    pub start_line: usize,
    pub total_lines: usize,
    pub cols: usize,
    pub running: bool,
    pub input_ready: bool,
    pub rows: Vec<String>,
    /// Full-page wrap flags, even when rows contains only a delta. Empty for older senders.
    #[serde(default)]
    pub soft_wrapped: Vec<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TerminalInput {
    Text { text: String },
    Enter,
    Tab,
    Escape,
    Backspace,
    Up,
    Down,
    Left,
    Right,
    CtrlC,
}
impl TerminalInput {
    pub fn valid(&self) -> bool {
        match self {
            Self::Text { text } => {
                !text.is_empty()
                    && text.len() <= 4096
                    && !text
                        .chars()
                        .any(|c| c.is_control() && c != '\n' && c != '\t')
            }
            _ => true,
        }
    }
}

/// Contiguous changed rows, applied only after stream/sequence and target validation.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TerminalEvent {
    pub stream_id: String,
    pub sequence: u64,
    pub request: TerminalRequest,
    pub page: Option<TerminalPage>,
    pub replace_from: usize,
    pub remove_count: usize,
}
pub struct TerminalProjection {
    stream_id: String,
    sequence: u64,
    request: TerminalRequest,
    last: Option<TerminalPage>,
}
impl TerminalProjection {
    pub fn new(stream_id: String, request: TerminalRequest) -> Self {
        Self {
            stream_id,
            request,
            sequence: 0,
            last: None,
        }
    }
    pub fn update(&mut self, page: TerminalPage) -> Result<TerminalEvent, &'static str> {
        let mut event = TerminalEvent {
            stream_id: self.stream_id.clone(),
            sequence: self.sequence,
            request: self.request.clone(),
            page: None,
            replace_from: 0,
            remove_count: 0,
        };
        if self.last.as_ref() == Some(&page) {
            return Ok(event);
        }
        let mut delta = page.clone();
        if let Some(last) = &self.last {
            self.sequence = self.sequence.checked_add(1).ok_or("unavailable")?;
            let prefix = last
                .rows
                .iter()
                .zip(&page.rows)
                .take_while(|(a, b)| a == b)
                .count();
            let suffix = last.rows[prefix..]
                .iter()
                .rev()
                .zip(page.rows[prefix..].iter().rev())
                .take_while(|(a, b)| a == b)
                .count();
            event.replace_from = prefix;
            event.remove_count = last.rows.len() - prefix - suffix;
            delta.rows = page.rows[prefix..page.rows.len() - suffix].to_vec();
        }
        event.sequence = self.sequence;
        event.page = Some(delta);
        self.last = Some(page);
        Ok(event)
    }
}
