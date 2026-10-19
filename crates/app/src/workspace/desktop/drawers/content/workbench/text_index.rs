//! Cached line index so the document list never rescans the whole file per frame.

/// Terminal-cell width: tabs show as four spaces and CJK glyphs take two cells.
pub(crate) fn display_width(line: &str) -> usize {
    line.chars()
        .map(|c| match c {
            '\t' => 4,
            c if (c as u32) >= 0x1100 && is_wide(c) => 2,
            _ => 1,
        })
        .sum()
}

fn is_wide(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x115F | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF
        | 0xFE30..=0xFE6F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6 | 0x20000..=0x3FFFD)
}

pub(crate) struct TextIndex {
    version: String,
    len: usize,
    ranges: Vec<(usize, usize)>,
    pub longest: Option<usize>,
}

impl TextIndex {
    pub fn new(version: &str, text: &str) -> Self {
        let mut ranges = Vec::new();
        let mut longest: Option<(usize, usize)> = None;
        let mut start = 0;
        for raw in text.split_inclusive('\n') {
            let end = start + raw.len();
            let line = raw.strip_suffix('\n').unwrap_or(raw);
            let line = line.strip_suffix('\r').unwrap_or(line);
            let width = display_width(line);
            if longest.is_none_or(|(_, w)| width > w) {
                longest = Some((ranges.len(), width));
            }
            ranges.push((start, start + line.len()));
            start = end;
        }
        Self {
            version: version.to_string(),
            len: text.len(),
            ranges,
            longest: longest.map(|(i, _)| i),
        }
    }

    pub fn matches(&self, version: &str, text: &str) -> bool {
        self.version == version && self.len == text.len()
    }

    /// Rows to render: an empty file still shows one blank numbered row.
    pub fn count(&self) -> usize {
        self.ranges.len().max(1)
    }

    pub fn line<'a>(&self, text: &'a str, index: usize) -> &'a str {
        self.ranges
            .get(index)
            .map_or("", |&(start, end)| &text[start..end])
    }
}
