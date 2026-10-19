//! Pure unified-diff parsing so the document pane can show line numbers and add/remove semantics.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiffKind {
    Add,
    Remove,
    Context,
    Hunk,
    Meta,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiffLine {
    pub kind: DiffKind,
    /// New-file number for added/context lines, old-file number for removed lines.
    pub number: Option<usize>,
    pub text: String,
}

fn hunk_starts(header: &str) -> Option<(usize, usize)> {
    let mut parts = header.split_whitespace();
    parts.next()?;
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    let start = |s: &str| s.split(',').next()?.parse::<usize>().ok();
    Some((start(old)?, start(new)?))
}

pub(crate) fn parse_diff(patch: &str) -> Vec<DiffLine> {
    let mut old = 0;
    let mut new = 0;
    let mut in_hunk = false;
    let mut lines = Vec::new();
    for raw in patch.lines() {
        let line = |kind, number, text: &str| DiffLine {
            kind,
            number,
            text: text.to_string(),
        };
        if raw.starts_with("@@") {
            if let Some((o, n)) = hunk_starts(raw) {
                old = o;
                new = n;
                in_hunk = true;
            }
            lines.push(line(DiffKind::Hunk, None, raw));
        } else if !in_hunk {
            lines.push(line(DiffKind::Meta, None, raw));
        } else if let Some(text) = raw.strip_prefix('+') {
            lines.push(line(DiffKind::Add, Some(new), text));
            new += 1;
        } else if let Some(text) = raw.strip_prefix('-') {
            lines.push(line(DiffKind::Remove, Some(old), text));
            old += 1;
        } else if raw.starts_with('\\') {
            lines.push(line(DiffKind::Meta, None, raw));
        } else {
            lines.push(line(
                DiffKind::Context,
                Some(new),
                raw.strip_prefix(' ').unwrap_or(raw),
            ));
            old += 1;
            new += 1;
        }
    }
    lines
}

pub(crate) fn diff_stats(lines: &[DiffLine]) -> (usize, usize) {
    let count = |k| lines.iter().filter(|l| l.kind == k).count();
    (count(DiffKind::Add), count(DiffKind::Remove))
}

pub(crate) fn is_binary_patch(patch: &str) -> bool {
    patch
        .lines()
        .any(|l| l.starts_with("Binary files ") || l.starts_with("GIT binary patch"))
}

/// Lines for the document pane: plumbing headers are dropped, rename/mode/binary notes stay.
pub(crate) fn parse_diff_for_display(patch: &str) -> Vec<DiffLine> {
    let mut lines = parse_diff(patch);
    lines.retain(|l| {
        l.kind != DiffKind::Meta
            || !["diff --git", "index ", "--- ", "+++ "]
                .iter()
                .any(|p| l.text.starts_with(p))
    });
    lines
}
