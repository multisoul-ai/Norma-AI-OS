//! Pure tree shaping for the explorer sidebar (changed files and filtered file search).

use super::FileRow;
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ChangeRow {
    Dir {
        path: String,
        label: String,
        depth: usize,
        open: bool,
    },
    File {
        index: usize,
        depth: usize,
        name: String,
    },
}

/// Sort key that lists directories before files at every level.
fn sort_key(path: &str, directory: bool) -> Vec<(bool, String, String)> {
    let parts = path.split('/').collect::<Vec<_>>();
    let last = parts.len() - 1;
    parts
        .iter()
        .enumerate()
        .map(|(i, p)| (i == last && !directory, p.to_lowercase(), p.to_string()))
        .collect()
}

fn ancestors(path: &str) -> Vec<String> {
    let parts = path.split('/').collect::<Vec<_>>();
    (1..parts.len()).map(|n| parts[..n].join("/")).collect()
}

fn label(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// Builds the "changed files" tree. Searching keeps matching files plus every ancestor and
/// ignores collapse state so a match can never be hidden.
pub(crate) fn change_rows(
    paths: &[String],
    collapsed: &BTreeSet<String>,
    query: &str,
    allow: Option<&[String]>,
) -> Vec<ChangeRow> {
    let needle = query.trim().to_lowercase();
    let searching = !needle.is_empty();
    let mut items = paths
        .iter()
        .enumerate()
        .filter(|(_, path)| {
            allow.is_none_or(|allow| allow.iter().any(|item| item == *path))
                && (!searching || path.to_lowercase().contains(&needle))
        })
        .collect::<Vec<_>>();
    items.sort_by_key(|(_, p)| sort_key(p, false));
    let mut emitted = BTreeSet::new();
    let mut rows = Vec::new();
    for (index, path) in items {
        let dirs = ancestors(path);
        let hidden_at = if searching {
            None
        } else {
            dirs.iter().position(|d| collapsed.contains(d))
        };
        let visible = hidden_at.map_or(dirs.len(), |i| i + 1);
        for (depth, dir) in dirs.iter().take(visible).enumerate() {
            if emitted.insert(dir.clone()) {
                rows.push(ChangeRow::Dir {
                    path: dir.clone(),
                    label: label(dir),
                    depth,
                    open: searching || !collapsed.contains(dir),
                });
            }
        }
        if hidden_at.is_none() {
            rows.push(ChangeRow::File {
                index,
                depth: dirs.len(),
                name: label(path),
            });
        }
    }
    rows
}

/// Turns flat search hits into a tree: ancestors are synthesized once, in path order.
pub(crate) fn search_rows(entries: &[(String, bool)]) -> Vec<FileRow> {
    let mut sorted = entries.to_vec();
    sorted.sort_by_key(|(p, d)| sort_key(p, *d));
    let mut dirs = BTreeSet::new();
    let mut rows = Vec::new();
    for (path, directory) in sorted {
        let parents = ancestors(&path);
        for (depth, dir) in parents.iter().enumerate() {
            if dirs.insert(dir.clone()) {
                rows.push(FileRow {
                    path: dir.clone(),
                    label: label(dir),
                    directory: true,
                    depth,
                });
            }
        }
        if directory && !dirs.insert(path.clone()) {
            continue;
        }
        rows.push(FileRow {
            label: label(&path),
            depth: parents.len(),
            path,
            directory,
        });
    }
    rows
}

/// A row's `+`/`−` button and click target act on the staged side only when nothing is left
/// unstaged; a partially staged (`MM`) file still offers "stage the rest".
pub(crate) fn row_is_staged(staged: bool, unstaged: bool) -> bool {
    staged && !unstaged
}
