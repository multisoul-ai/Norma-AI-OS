//! Agent 路径输出约定：反引号包裹的路径，可含空格，可被终端软折行拆成多行。

use super::path_ref::{parse_path_ref, PathRefSpan, PathTier};
use super::{Cell, GridSnapshot};

fn soft_wraps(row: &[Cell]) -> bool {
    row.last().is_some_and(|cell| cell.attrs.soft_wrap)
}

/// `row` 所在逻辑行（经软折行连接的所有可视行）里的反引号强路径，只返回落在 `row` 上的区间。
pub(super) fn spans_for_row(rows: &[Vec<Cell>], row: usize) -> Vec<PathRefSpan> {
    if row >= rows.len() {
        return Vec::new();
    }
    let mut first = row;
    while first > 0 && soft_wraps(&rows[first - 1]) {
        first -= 1;
    }
    let mut last = row;
    while last + 1 < rows.len() && soft_wraps(&rows[last]) {
        last += 1;
    }
    // 逻辑行字符及其所在 (行, 列)；宽字符占位格不产生字符。
    let mut chars: Vec<(char, usize, usize)> = Vec::new();
    for (r, cells) in rows.iter().enumerate().take(last + 1).skip(first) {
        for (c, cell) in cells.iter().enumerate() {
            if !cell.attrs.wide_char_spacer {
                chars.push((cell.c, r, c));
            }
        }
    }
    let mut spans = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index].0 != '`' {
            index += 1;
            continue;
        }
        let Some(close) = (index + 1..chars.len()).find(|&i| chars[i].0 == '`') else {
            break;
        };
        let inner = &chars[index + 1..close];
        let text: String = inner.iter().map(|(ch, _, _)| *ch).collect();
        index = close + 1;
        let crosses_rows = inner
            .first()
            .zip(inner.last())
            .is_some_and(|(a, b)| a.1 != b.1);
        if !(text.contains(' ') || crosses_rows) {
            continue;
        }
        let Some(path_ref) = parse_path_ref(&text) else {
            continue;
        };
        if path_ref.tier != PathTier::Strong || path_ref.raw != text {
            continue;
        }
        let on_row: Vec<_> = inner.iter().filter(|(_, r, _)| *r == row).collect();
        let (Some(head), Some(tail)) = (on_row.first(), on_row.last()) else {
            continue;
        };
        let mut end = tail.2 + 1;
        while rows[row]
            .get(end)
            .is_some_and(|cell| cell.attrs.wide_char_spacer)
        {
            end += 1;
        }
        spans.push(PathRefSpan {
            start: head.2,
            end,
            path_ref,
        });
    }
    spans
}

impl GridSnapshot {
    /// 命中单元格所在的反引号约定路径（含软折行与空格）；它是 Agent 明确声明的完整路径，
    /// 点击时应优先于折行启发式给出的残段候选。
    pub fn backtick_path_at(&self, row: usize, col: usize) -> Option<super::path_ref::PathRef> {
        spans_for_row(&self.rows, row)
            .into_iter()
            .find(|span| col >= span.start && col < span.end)
            .map(|span| span.path_ref)
    }
}
