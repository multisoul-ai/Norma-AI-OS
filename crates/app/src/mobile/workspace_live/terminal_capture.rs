use super::*;
use crate::cards::term::paint_style::{resolve_cell_style, TerminalPalette};
use norma::mobile::{
    TerminalCursor, WorkspaceTerminalCell, WorkspaceTerminalFrame, WorkspaceTerminalRequest,
};
use std::hash::{Hash, Hasher};

struct Capture {
    frame: WorkspaceTerminalFrame,
    live_rows: Vec<Vec<terminal::Cell>>,
    history_rows: Option<Vec<Vec<terminal::Cell>>>,
    anchor_rows: Vec<Vec<terminal::Cell>>,
    boundary_rows: Vec<Vec<terminal::Cell>>,
    before: Option<usize>,
    palette: TerminalPalette,
    history_version: Option<String>,
}

/// Read an already-running retained canvas. Viewing never loads a project or starts a PTY.
pub(super) fn start(database: PathBuf, call: LiveWorkspaceRequest, cx: &mut App) {
    let LiveWorkspaceAction::Terminal { request } = &call.action else {
        return;
    };
    let captured = capture(&database, &call.areas, request, cx);
    cx.spawn(async move |cx| {
        let result = match captured {
            Ok(Some(captured)) => {
                cx.background_executor()
                    .spawn(async move { encode(captured) })
                    .await
            }
            Ok(None) => Ok(Value::Null),
            Err(error) => Err(error),
        };
        let _ = call.reply.try_send(result);
    })
    .detach();
}
fn capture(
    database: &Path,
    areas: &[WorkspaceArea],
    request: &WorkspaceTerminalRequest,
    cx: &App,
) -> Result<Option<Capture>, String> {
    let bound = areas
        .iter()
        .find(|a| {
            a.area.id == request.target.area_id
                && a.area.desktop_workspace_id == request.desktop_workspace_id
        })
        .ok_or("target_missing")?;
    let session = crate::workspace::desktop::mobile::session(database, &bound.area, cx)
        .ok_or("terminal_unavailable")?;
    let app = session.read(cx);
    let target = &request.target;
    let card = app
        .mobile_card_snapshots()
        .into_iter()
        .find(|c| c.node_id == target.node_id)
        .ok_or("target_missing")?;
    let definition = app
        .runtime
        .document
        .node(target.node_id)
        .and_then(|node| match &node.kind {
            canvas::CanvasNodeKind::Agent(spec) => spec.definition_id,
            _ => None,
        })
        .ok_or("target_missing")?;
    target
        .validate_card(&bound.area.id, definition, &card)
        .map_err(str::to_owned)?;
    // Continuations validate the live identity, then read the immutable cached capture.
    // New output must not invalidate a history fragment that was already authorized.
    if request.snapshot_id.is_some() {
        return Ok(None);
    }
    let runtime::NodeView::Term {
        snapshot, status, ..
    } = app
        .runtime
        .node_view(target.node_id)
        .ok_or("terminal_unavailable")?;
    let total = snapshot.history_size + snapshot.lines;
    if total == 0 || total > 100_000 || snapshot.cols > 512 || snapshot.lines > 120 {
        return Err("terminal_unavailable".into());
    }
    let start = total.saturating_sub(120);
    let live_rows = app
        .runtime
        .node_terminal_rows(target.node_id, start, total - 1)
        .ok_or("terminal_unavailable")?;
    let history_rows = if let Some(before) = request.before {
        if before > total {
            return Err("terminal_changed".into());
        }
        Some(
            app.runtime
                .node_terminal_rows(target.node_id, before.saturating_sub(120), before - 1)
                .ok_or("terminal_unavailable")?,
        )
    } else {
        None
    };
    let page_start = request.before.unwrap_or(total).saturating_sub(120);
    let anchor_rows = anchor(&app.runtime, target.node_id, page_start)?;
    let boundary_rows = anchor(
        &app.runtime,
        target.node_id,
        request.before.unwrap_or(start),
    )?;
    Ok(Some(Capture {
        frame: WorkspaceTerminalFrame {
            version: String::new(),
            cols: snapshot.cols,
            screen_lines: snapshot.lines,
            start_line: start,
            total_lines: total,
            running: status == terminal::TerminalStatus::Running,
            alt_screen: snapshot.alt_screen,
            cursor: snapshot.show_cursor.then_some(TerminalCursor {
                line: snapshot.history_size + snapshot.cursor_line,
                column: snapshot.cursor_col,
            }),
            background: app.theme.term_bg >> 8,
            foreground: app.theme.term_fg >> 8,
            rows: Vec::new(),
        },
        live_rows,
        history_rows,
        anchor_rows,
        boundary_rows,
        before: request.before,
        palette: TerminalPalette::from_theme(app.theme),
        history_version: request.history_version.clone(),
    }))
}
fn anchor(
    runtime: &runtime::Runtime,
    node: canvas::NodeId,
    before: usize,
) -> Result<Vec<Vec<terminal::Cell>>, String> {
    if before == 0 {
        return Ok(Vec::new());
    }
    // Eight rows immediately before the page boundary detect shifted/cleared scrollback.
    // Unlike a live-screen hash, these rows stay stable during progress redraws and appends.
    runtime
        .node_terminal_rows(node, before.saturating_sub(8), before - 1)
        .ok_or_else(|| "terminal_unavailable".into())
}
fn history_key(
    frame: &WorkspaceTerminalFrame,
    before: usize,
    rows: Vec<Vec<terminal::Cell>>,
    palette: TerminalPalette,
) -> Result<String, String> {
    let metadata = (
        frame.cols,
        frame.screen_lines,
        frame.alt_screen,
        before,
        cells(rows, palette),
    );
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_vec(&metadata)
        .map_err(|_| "unavailable")?
        .hash(&mut hash);
    Ok(format!("{:016x}", hash.finish()))
}
fn encode(mut capture: Capture) -> Result<Value, String> {
    if let Some(expected) = &capture.history_version {
        let boundary = history_key(
            &capture.frame,
            capture.before.ok_or("invalid_history")?,
            capture.boundary_rows,
            capture.palette,
        )?;
        if expected.split('-').next() != Some(boundary.as_str()) {
            return Err("terminal_changed".into());
        }
    }
    if let Some(rows) = capture.history_rows {
        capture.frame.rows = cells(rows, capture.palette);
        capture.frame.start_line = capture.before.unwrap_or_default().saturating_sub(120);
        capture.frame.cursor = None;
    } else {
        capture.frame.rows = cells(capture.live_rows, capture.palette);
    }
    let anchor = history_key(
        &capture.frame,
        capture.frame.start_line,
        capture.anchor_rows,
        capture.palette,
    )?;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_vec(&capture.frame)
        .map_err(|_| "unavailable")?
        .hash(&mut hash);
    // The first component anchors the next history page; the second versions this whole frame.
    capture.frame.version = format!("{anchor}-{:016x}", hash.finish());
    serde_json::to_value(capture.frame).map_err(|_| "unavailable".into())
}
fn cells(
    rows: Vec<Vec<terminal::Cell>>,
    palette: TerminalPalette,
) -> Vec<Vec<WorkspaceTerminalCell>> {
    rows.into_iter()
        .map(|row| {
            row.into_iter()
                .map(|cell| {
                    let style = resolve_cell_style(&cell, palette);
                    let a = cell.attrs;
                    WorkspaceTerminalCell {
                        text: cell.c.to_string(),
                        fg: rgb(style.fg),
                        bg: rgb(style.bg),
                        flags: u16::from(a.bold)
                            | (u16::from(a.italic) << 1)
                            | (u16::from(a.underline) << 2)
                            | (u16::from(a.strikeout) << 3)
                            | (u16::from(a.hidden) << 4)
                            | (u16::from(a.wide_char_spacer) << 5)
                            | (u16::from(a.soft_wrap) << 6),
                    }
                })
                .collect()
        })
        .collect()
}
fn rgb(color: gpui::Hsla) -> u32 {
    u32::from(gpui::Rgba::from(color)) >> 8
}

#[cfg(test)]
#[path = "../../../tests/unit/workspace_terminal_capture_tests.rs"]
mod tests;
