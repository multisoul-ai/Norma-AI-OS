use super::*;
use crate::{NodeInput, NodeView};
use norma::mobile::{CardSnapshot, TerminalInput, TerminalPage, TerminalRequest};

impl Runtime {
    pub fn mobile_terminal_page(
        &self,
        request: &TerminalRequest,
    ) -> Result<TerminalPage, &'static str> {
        if !request.valid() {
            return Err("invalid_request");
        }
        let card = self
            .mobile_card_snapshots()
            .into_iter()
            .find(|card| card.node_id == request.node_id)
            .ok_or("session_changed")?;
        if card.run_id.as_deref() != Some(&request.run_id)
            || request
                .session_id
                .as_ref()
                .is_some_and(|id| card.session_id.as_ref() != Some(id))
        {
            return Err("session_changed");
        }
        let run = self
            .mobile_runs
            .get(&card.node_id)
            .filter(|run| run.native)
            .ok_or("terminal_unavailable")?;
        let NodeView::Term {
            snapshot,
            revision,
            status,
        } = self.node_view(card.node_id).ok_or("terminal_unavailable")?;
        if request.revision.is_some_and(|r| r != revision) {
            return Err("terminal_changed");
        }
        let total = snapshot.history_size + snapshot.lines;
        let end = request.before.unwrap_or(total);
        if end == 0 || end > total || total > 100_000 || snapshot.cols > 512 || snapshot.lines > 120
        {
            return Err("terminal_unavailable");
        }
        //120 rows ×512 columns ×4 UTF-8 bytes <=240KiB; serialized controls still fit a512KiB frame.
        let start = end.saturating_sub(120);
        let cells = self
            .node_terminal_rows(card.node_id, start, end - 1)
            .ok_or("terminal_unavailable")?;
        let soft_wrapped: Vec<bool> = cells
            .iter()
            .map(|row| row.last().is_some_and(|cell| cell.attrs.soft_wrap))
            .collect();
        let rows = cells
            .iter()
            .zip(&soft_wrapped)
            .map(|(row, wrapped)| {
                let text = row
                    .iter()
                    .filter(|c| !c.attrs.wide_char_spacer)
                    .map(|c| c.c)
                    .collect::<String>();
                // Spaces at a soft boundary belong to the logical line, not grid padding.
                if *wrapped {
                    text
                } else {
                    text.trim_end().to_owned()
                }
            })
            .collect();
        let running = status == TerminalStatus::Running;
        Ok(TerminalPage {
            screen_signature: self.mobile_terminal_signature(card.node_id)?,
            card,
            revision,
            input_sequence: run.native_input.sequence,
            start_line: start,
            total_lines: total,
            cols: snapshot.cols,
            cursor: (request.before.is_none() && snapshot.show_cursor).then_some(
                norma::mobile::TerminalCursor {
                    line: snapshot.history_size + snapshot.cursor_line,
                    column: snapshot.cursor_col,
                },
            ),
            rows,
            soft_wrapped,
            running,
            input_ready: running
                && request.before.is_none()
                && terminal_input_ready(run)
                && !self.mobile_terminal_is_controlled(request.node_id),
        })
    }

    // A precondition, not a credential: visual content/cursor and the original input counter
    // detect competing edits. Redrawing the same screen or blinking its cursor must not reject input.
    fn mobile_terminal_signature(&self, node: NodeId) -> Result<u64, &'static str> {
        Ok(self.native_terminal_frame(node)?.signature)
    }

    /// Explicit user terminal input: the receipt means bytes written, never task/approval completion.
    pub fn send_mobile_terminal_input(
        &mut self,
        expected: &CardSnapshot,
        screen_signature: u64,
        input_sequence: u64,
        input: &TerminalInput,
    ) -> Result<(), &'static str> {
        self.send_terminal_input(expected, screen_signature, input_sequence, input, None)
    }

    pub(super) fn send_terminal_input(
        &mut self,
        expected: &CardSnapshot,
        screen_signature: u64,
        input_sequence: u64,
        input: &TerminalInput,
        owner: Option<(&storage::MobileTurnKey, &Arc<super::TerminalInputLease>)>,
    ) -> Result<(), &'static str> {
        if let Some((key, lease)) = owner {
            self.validate_terminal_input_owner(key, lease)?;
        } else if self.mobile_terminal_is_controlled(expected.node_id) {
            return Err("terminal_controlled");
        }
        if !input.valid() {
            return Err("invalid_request");
        }
        let current = self
            .mobile_card_snapshots()
            .into_iter()
            .find(|c| c.node_id == expected.node_id)
            .ok_or("session_changed")?;
        if !current.mobile_control
            || current.profile != expected.profile
            || current.run_id.is_none()
            || current.run_id != expected.run_id
            || current.session_id != expected.session_id
            || current.turn_id != expected.turn_id
        {
            return Err("session_changed");
        }
        let run = self
            .mobile_runs
            .get(&current.node_id)
            .ok_or("terminal_unavailable")?;
        if !run.native || !terminal_input_ready(run) {
            return Err("state_not_ready");
        }
        let NodeView::Term { status, .. } = self
            .node_view(current.node_id)
            .ok_or("terminal_unavailable")?;
        if status != TerminalStatus::Running {
            return Err("terminal_unavailable");
        }
        if self.mobile_terminal_signature(current.node_id)? != screen_signature
            || run.native_input.sequence != input_sequence
            || input_sequence == u64::MAX
        {
            return Err("terminal_changed");
        }
        let input = match input {
            TerminalInput::Text { text } => {
                if text.contains(['\n', '\t'])
                    && !self
                        .live
                        .get(&current.node_id)
                        .is_some_and(|live| live.terminal_bracketed_paste_enabled())
                {
                    return Err("paste_unavailable");
                }
                NodeInput::Paste(text.clone())
            }
            key => NodeInput::Key(terminal::KeyInput {
                key: match key {
                    TerminalInput::Enter => terminal::Key::Enter,
                    TerminalInput::Tab => terminal::Key::Tab,
                    TerminalInput::Escape => terminal::Key::Escape,
                    TerminalInput::Backspace => terminal::Key::Backspace,
                    TerminalInput::Up => terminal::Key::Up,
                    TerminalInput::Down => terminal::Key::Down,
                    TerminalInput::Left => terminal::Key::Left,
                    TerminalInput::Right => terminal::Key::Right,
                    TerminalInput::CtrlC => terminal::Key::Char('c'),
                    TerminalInput::Text { .. } => unreachable!("text handled above"),
                },
                ctrl: matches!(key, TerminalInput::CtrlC),
                alt: false,
                shift: false,
            }),
        };
        if let Some((key, lease)) = owner {
            self.validate_terminal_input_owner(key, lease)?;
            // Mark attempted owner input too: a partial PTY write may have left human text.
            self.mobile_runs
                .get_mut(&key.node_id)
                .expect("validated owner")
                .native_input
                .controlled_draft = true;
        }
        self.stop_cursor_question_input(current.node_id);
        self.write_node_input(current.node_id, input)
            .map_err(|_| "outcome_unknown")
    }
}
pub(super) fn terminal_input_ready(run: &MobileRun) -> bool {
    run.message.is_none()
        && run.interrupt.is_none()
        && run
            .native_approvals
            .permission
            .as_ref()
            .is_none_or(|p| !p.claimed)
        && run.native_input.sequence < u64::MAX
}
