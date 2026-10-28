//! Live viewport and a one-step input lease, independent of the mobile feature switch.
use super::*;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

pub(super) struct NativeTerminalFrame {
    pub signature: u64,
    pub input_sequence: u64,
    pub rows: Vec<String>,
}

pub(super) struct TerminalStepLease {
    sequence: u64,
    signature: u64,
    stable_since: Instant,
    sent: Option<(u64, Instant)>,
    revoked: Option<&'static str>,
}
impl TerminalStepLease {
    pub fn new(frame: &NativeTerminalFrame, now: Instant) -> Self {
        Self {
            sequence: frame.input_sequence,
            signature: frame.signature,
            stable_since: now,
            sent: None,
            revoked: None,
        }
    }
    pub fn ready(
        &mut self,
        frame: &NativeTerminalFrame,
        now: Instant,
    ) -> Result<bool, &'static str> {
        if frame.input_sequence != self.sequence {
            self.revoked = Some("question_native_takeover");
        }
        if let Some(reason) = self.revoked {
            return Err(reason);
        }
        if self.signature != frame.signature {
            self.signature = frame.signature;
            self.stable_since = now;
        }
        if let Some((signature, at)) = self.sent {
            if signature == frame.signature {
                if now.saturating_duration_since(at) >= Duration::from_secs(5) {
                    self.revoked = Some("question_terminal_no_progress");
                    return Err("question_terminal_no_progress");
                }
                return Ok(false);
            }
        }
        Ok(now.saturating_duration_since(self.stable_since) >= Duration::from_millis(500))
    }
    pub fn written(&mut self, frame: &NativeTerminalFrame, sequence: u64, now: Instant) {
        self.sequence = sequence;
        self.sent = Some((frame.signature, now));
        self.stable_since = now;
    }
}

impl Runtime {
    pub(super) fn native_terminal_frame(
        &self,
        node: NodeId,
    ) -> Result<NativeTerminalFrame, &'static str> {
        let NodeView::Term { snapshot, .. } = self.node_view(node).ok_or("terminal_unavailable")?;
        if snapshot.lines == 0 || snapshot.cols > 512 || snapshot.lines > 120 {
            return Err("terminal_unavailable");
        }
        let input_sequence = self
            .mobile_runs
            .get(&node)
            .and_then(|run| run.native_input_sequence())
            .ok_or("terminal_unavailable")?;
        if input_sequence == u64::MAX {
            return Err("terminal_unavailable");
        }
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        (
            snapshot.cols,
            snapshot.lines,
            snapshot.alt_screen,
            snapshot.cursor_col,
            snapshot.cursor_line,
        )
            .hash(&mut hash);
        let cells = self
            .node_terminal_rows(
                node,
                snapshot.history_size,
                snapshot.history_size + snapshot.lines - 1,
            )
            .ok_or("terminal_unavailable")?;
        let mut rows = Vec::with_capacity(cells.len());
        for row in cells {
            for cell in &row {
                cell.c.hash(&mut hash);
            }
            rows.push(
                row.iter()
                    .filter(|cell| !cell.attrs.wide_char_spacer)
                    .map(|cell| cell.c)
                    .collect::<String>()
                    .trim_end()
                    .to_owned(),
            );
        }
        Ok(NativeTerminalFrame {
            signature: hash.finish(),
            input_sequence,
            rows,
        })
    }
}

#[cfg(test)]
#[path = "../../tests/unit/terminal_interaction_tests.rs"]
mod tests;
