//! Pure presentation data for recent workspaces and the searchable directory.
#[derive(Clone, Debug)]
pub(super) struct HomeEntry {
    pub id: String,
    pub name: String,
    pub project_count: usize,
    pub opened_ms: i64,
    pub running: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum GroupKind {
    Running,
    Recent,
    Older,
    Matches,
}

pub(super) struct HomeGroup {
    pub kind: GroupKind,
    pub total: usize,
    pub entries: Vec<HomeEntry>,
}

fn sorted(entries: &[HomeEntry]) -> Vec<HomeEntry> {
    let mut entries = entries.to_vec();
    entries.sort_by(|a, b| b.opened_ms.cmp(&a.opened_ms).then_with(|| a.id.cmp(&b.id)));
    entries
}

pub(super) fn recent(entries: &[HomeEntry]) -> Vec<HomeEntry> {
    sorted(entries).into_iter().take(4).collect()
}

pub(super) fn groups(entries: &[HomeEntry], query: &str, more: bool, now: i64) -> Vec<HomeGroup> {
    let query = query.trim().to_lowercase();
    let entries: Vec<_> = sorted(entries)
        .into_iter()
        .filter(|entry| entry.name.to_lowercase().contains(&query))
        .collect();
    if !query.is_empty() {
        return if entries.is_empty() {
            Vec::new()
        } else {
            vec![HomeGroup {
                kind: GroupKind::Matches,
                total: entries.len(),
                entries,
            }]
        };
    }
    [GroupKind::Running, GroupKind::Recent, GroupKind::Older]
        .into_iter()
        .filter_map(|kind| {
            let mut items: Vec<_> = entries
                .iter()
                .filter(|entry| {
                    let recent = now.saturating_sub(entry.opened_ms) <= 7 * 86_400_000;
                    match kind {
                        GroupKind::Running => entry.running,
                        GroupKind::Recent => !entry.running && recent,
                        GroupKind::Older => !entry.running && !recent,
                        GroupKind::Matches => false,
                    }
                })
                .cloned()
                .collect();
            let total = items.len();
            if total == 0 {
                return None;
            }
            if kind == GroupKind::Older && !more && total > 3 {
                items.truncate(2);
            }
            Some(HomeGroup {
                kind,
                total,
                entries: items,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "../../../../tests/unit/desktop_home_catalog_tests.rs"]
mod tests;

pub(super) fn highlight_ranges(name: &str, query: &str) -> Vec<std::ops::Range<usize>> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let mut folded = String::new();
    let mut original = Vec::new();
    for (start, ch) in name.char_indices() {
        let lower = ch.to_lowercase().to_string();
        original.extend(std::iter::repeat_n(
            start..start + ch.len_utf8(),
            lower.len(),
        ));
        folded.push_str(&lower);
    }
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    for (start, hit) in folded.match_indices(&query) {
        let range = original[start].start..original[start + hit.len() - 1].end;
        if let Some(previous) = ranges
            .last_mut()
            .filter(|previous| previous.end > range.start)
        {
            previous.end = previous.end.max(range.end);
        } else {
            ranges.push(range);
        }
    }
    ranges
}
