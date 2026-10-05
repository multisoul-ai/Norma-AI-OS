//! Read-only, styled grid snapshots. Fragments always belong to one immutable capture.
use super::{workspace_chat::validate_id, TerminalCursor, WorkspaceAgentTarget};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceTerminalRequest {
    pub desktop_workspace_id: String,
    pub target: WorkspaceAgentTarget,
    pub known_version: Option<String>,
    pub before: Option<usize>,
    pub history_version: Option<String>,
    pub snapshot_id: Option<String>,
    pub offset: Option<usize>,
}
impl WorkspaceTerminalRequest {
    pub fn validate(&self) -> Result<()> {
        validate_id(&self.desktop_workspace_id)?;
        validate_id(&self.target.area_id)?;
        ensure!(
            self.target.node_id > 0 && self.target.definition_id > 0,
            "invalid_target"
        );
        validate_id(
            self.target
                .run_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("run_missing"))?,
        )?;
        for id in [
            &self.target.session_id,
            &self.known_version,
            &self.history_version,
            &self.snapshot_id,
        ]
        .into_iter()
        .flatten()
        {
            validate_id(id)?;
        }
        ensure!(
            self.before.is_some() == self.history_version.is_some(),
            "invalid_history"
        );
        ensure!(
            self.before.is_none_or(|n| n > 0 && n <= 100_000),
            "invalid_history"
        );
        ensure!(
            self.snapshot_id.is_some() == self.offset.is_some(),
            "invalid_fragment"
        );
        ensure!(
            self.offset.is_none_or(|n| n > 0 && n < 120),
            "invalid_fragment"
        );
        Ok(())
    }
}

/// Flags: bold=1, italic=2, underline=4, strikeout=8, hidden=16, wide spacer=32,
/// soft wrap=64. Colors are resolved by the same palette as the desktop renderer.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceTerminalCell {
    pub text: String,
    pub fg: u32,
    pub bg: u32,
    pub flags: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceTerminalFrame {
    pub version: String,
    pub cols: usize,
    pub screen_lines: usize,
    pub start_line: usize,
    pub total_lines: usize,
    pub running: bool,
    pub alt_screen: bool,
    pub cursor: Option<TerminalCursor>,
    pub background: u32,
    pub foreground: u32,
    pub rows: Vec<Vec<WorkspaceTerminalCell>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceTerminalFragment {
    pub snapshot_id: String,
    pub row_count: usize,
    pub offset: usize,
    pub next_offset: Option<usize>,
    #[serde(flatten)]
    pub frame: WorkspaceTerminalFrame,
}
impl std::ops::Deref for WorkspaceTerminalFragment {
    type Target = WorkspaceTerminalFrame;
    fn deref(&self) -> &Self::Target {
        &self.frame
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceTerminalUpdate {
    pub version: String,
    pub fragment: Option<WorkspaceTerminalFragment>,
}
impl WorkspaceTerminalFrame {
    pub fn fragment(&self, id: &str, offset: usize) -> Result<WorkspaceTerminalFragment> {
        ensure!(
            offset < self.rows.len() && self.rows.len() <= 120,
            "invalid_cursor"
        );
        let mut fragment = WorkspaceTerminalFragment {
            snapshot_id: id.into(),
            row_count: self.rows.len(),
            offset,
            next_offset: None,
            frame: self.clone(),
        };
        fragment.frame.rows.clear();
        // null is at least as long as a bounded continuation offset (1..119).
        // Account for each row once, instead of repeatedly encoding a growing frame.
        let mut bytes = serde_json::to_vec(&fragment)?.len();
        for row in &self.rows[offset..] {
            // 200 KiB leaves 56 KiB for the wire envelope and relay encryption (24 bytes).
            let row_bytes =
                serde_json::to_vec(row)?.len() + usize::from(!fragment.frame.rows.is_empty());
            if bytes + row_bytes > 200 * 1024 {
                break;
            }
            bytes += row_bytes;
            fragment.frame.rows.push(row.clone());
        }
        ensure!(!fragment.frame.rows.is_empty(), "terminal_row_too_large");
        let end = offset + fragment.frame.rows.len();
        fragment.next_offset = (end < self.rows.len()).then_some(end);
        Ok(fragment)
    }
}
