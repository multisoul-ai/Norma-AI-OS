//! 终端文本里的文件路径引用：识别 `path[:行[:列]]`，不做任何文件系统 IO。

use super::{Cell, GridSnapshot};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathTier {
    /// 含目录与扩展名，可直接作为点击目标。
    Strong,
    /// 形似路径但误报概率高（裸文件名、无扩展名的绝对形态）；须后台验证存在后才可点击。
    NeedsVerify,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathRef {
    /// 终端里用户看到的文本（已去包裹符号，含行列后缀）。
    pub raw: String,
    /// 去掉行列后缀的路径；`~` 与相对路径留给 app 解析层处理。
    pub path: String,
    pub line: Option<u32>,
    pub col: Option<u32>,
    pub tier: PathTier,
}

/// 一行内的路径区间，`end` 为右开列号。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathRefSpan {
    pub start: usize,
    pub end: usize,
    pub path_ref: PathRef,
}

const LEADING_WRAPPERS: &[char] = &['(', '[', '{', '<', '"', '\'', '`'];
const TRAILING_WRAPPERS: &[char] = &[
    ')', ']', '}', '>', '"', '\'', '`', '.', ',', ';', ':', '!', '?',
];

pub fn parse_path_ref(text: &str) -> Option<PathRef> {
    let raw = text
        .trim_start_matches(LEADING_WRAPPERS)
        .trim_end_matches(TRAILING_WRAPPERS);
    if raw.is_empty() || (raw.contains("://") && !raw.starts_with("file://")) {
        return None;
    }
    let (path, line, col) = split_line_col(raw);
    if path.is_empty() {
        return None;
    }
    let tier = classify(path)?;
    Some(PathRef {
        raw: raw.to_string(),
        path: path.to_string(),
        line,
        col,
        tier,
    })
}

/// 反复剥离末尾 `:数字`，最多两层：先得到列再得到行。
fn split_line_col(raw: &str) -> (&str, Option<u32>, Option<u32>) {
    let strip = |text: &str| -> Option<(usize, u32)> {
        let (head, digits) = text.rsplit_once(':')?;
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some((head.len(), digits.parse().ok()?))
    };
    let Some((head_len, last)) = strip(raw) else {
        return (raw, None, None);
    };
    let head = &raw[..head_len];
    match strip(head) {
        Some((path_len, line)) => (&head[..path_len], positive(line), positive(last)),
        None => (head, positive(last), None),
    }
}

fn positive(value: u32) -> Option<u32> {
    (value > 0).then_some(value)
}

fn classify(path: &str) -> Option<PathTier> {
    let is_file_uri = path.starts_with("file://");
    let has_dir = path.contains('/');
    let last_segment = path.rsplit('/').next().unwrap_or_default();
    let has_extension = has_extension(last_segment);
    if (has_dir || is_file_uri) && has_extension {
        return Some(PathTier::Strong);
    }
    if has_extension && !is_file_uri {
        return Some(PathTier::NeedsVerify);
    }
    let rooted = path.starts_with(['/', '~']) || path.starts_with("./") || path.starts_with("../");
    (rooted && !last_segment.is_empty()).then_some(PathTier::NeedsVerify)
}

fn has_extension(segment: &str) -> bool {
    let Some((_, extension)) = segment.rsplit_once('.') else {
        return false;
    };
    (1..=12).contains(&extension.len())
        && extension.starts_with(|c: char| c.is_ascii_alphabetic())
        && extension
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-'))
}

impl GridSnapshot {
    /// 单行内所有路径引用的列区间。网页链接与带 OSC 8 的非 file 文本不参与。
    ///
    /// 只扫描单行（不跨折行拼接），以便渲染路径按行缓存；跨行路径仍可点击，但没有下划线。
    pub fn path_ref_spans_for_row(&self, row: usize) -> Vec<PathRefSpan> {
        let Some(cells) = self.rows.get(row) else {
            return Vec::new();
        };
        let mut spans = Vec::new();
        let mut index = 0;
        while index < cells.len() {
            if is_blank(&cells[index]) {
                index += 1;
                continue;
            }
            let token_end = cells[index..]
                .iter()
                .position(is_blank)
                .map_or(cells.len(), |offset| index + offset);
            if let Some(span) = token_span(cells, index, token_end) {
                spans.push(span);
            }
            index = token_end;
        }
        spans
    }

    /// 命中单元格所在的路径引用（任意 tier）；由调用方决定 NeedsVerify 是否已验证。
    pub fn path_ref_at(&self, row: usize, col: usize) -> Option<PathRef> {
        self.path_ref_spans_for_row(row)
            .into_iter()
            .find(|span| col >= span.start && col < span.end)
            .map(|span| span.path_ref)
    }
}

fn is_blank(cell: &Cell) -> bool {
    cell.c.is_whitespace() && !cell.attrs.wide_char_spacer
}

fn token_span(cells: &[Cell], start: usize, end: usize) -> Option<PathRefSpan> {
    if cells[start..end]
        .iter()
        .any(|cell| cell.hyperlink.is_some())
    {
        return None;
    }
    let mut left = start;
    while left < end && LEADING_WRAPPERS.contains(&cells[left].c) {
        left += 1;
    }
    let mut right = end;
    while right > left && TRAILING_WRAPPERS.contains(&cells[right - 1].c) {
        right -= 1;
    }
    let text: String = cells[left..right]
        .iter()
        .filter(|cell| !cell.attrs.wide_char_spacer)
        .map(|cell| cell.c)
        .collect();
    let path_ref = parse_path_ref(&text)?;
    Some(PathRefSpan {
        start: left,
        end: right,
        path_ref,
    })
}

#[cfg(test)]
#[path = "../../tests/unit/path_ref_tests.rs"]
mod tests;
