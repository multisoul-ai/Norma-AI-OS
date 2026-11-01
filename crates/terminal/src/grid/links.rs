use super::path_ref::{cut_at_fullwidth_punct, fullwidth_segments, parse_path_ref, PathTier};
use super::{Cell, GridSnapshot};

struct BareWebUrl {
    start: usize,
    end: usize,
    url: String,
}

impl GridSnapshot {
    /// 返回指定可见单元格的 OSC 8 目标 URI。
    pub fn hyperlink_at(&self, row: usize, col: usize) -> Option<&str> {
        self.rows
            .get(row)?
            .get(col)?
            .hyperlink
            .as_ref()
            .map(super::TerminalHyperlink::uri)
    }

    /// 返回指定可见单元格可安全打开的网页链接。
    ///
    /// OSC 8 链接优先；普通终端文本则只识别 `http://` 与 `https://`。
    pub fn link_at(&self, row: usize, col: usize) -> Option<String> {
        if let Some(target) = self.hyperlink_at(row, col) {
            return is_valid_web_url(target).then(|| target.to_string());
        }
        bare_web_urls(self.rows.get(row)?)
            .into_iter()
            .find(|link| col >= link.start && col < link.end)
            .map(|link| link.url)
    }

    /// 返回某一行中所有可安全打开链接的单元格掩码，供渲染一次扫描复用。
    pub fn link_columns_for_row(&self, row: usize) -> Vec<bool> {
        let Some(cells) = self.rows.get(row) else {
            return Vec::new();
        };
        let mut linked = cells
            .iter()
            .map(|cell| {
                cell.hyperlink
                    .as_ref()
                    .map(|target| is_valid_web_url(target.uri()))
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>();
        for link in bare_web_urls(cells) {
            for column in link.start..link.end {
                if cells[column].hyperlink.is_none() {
                    linked[column] = true;
                }
            }
        }
        linked
    }

    /// 指定单元格是否属于看起来像 URI、但因协议或格式不安全而不能打开的文本。
    ///
    /// UI 用它保留文本选择行为，同时给出简短的拒绝反馈；实际打开仍必须只使用
    /// [`Self::link_at`] 的返回值。
    pub fn has_link_candidate_at(&self, row: usize, col: usize) -> bool {
        self.hyperlink_at(row, col).is_some()
            || self
                .rows
                .get(row)
                .map(Vec::as_slice)
                .map(|cells| bare_uri_candidate_at(cells, col))
                .unwrap_or(false)
    }

    /// 返回可预览的本机文件目标；具体路径解析与文件校验由 app 后台加载链路负责。
    pub fn preview_path_at(&self, row: usize, col: usize) -> Option<String> {
        if let Some(target) = self.hyperlink_at(row, col) {
            return (target.starts_with("file://") && is_preview_path_candidate(target))
                .then(|| target.to_string());
        }
        let token = wrapped_terminal_token_at(&self.rows, row, col)?;
        parse_path_ref(&token)
            .filter(|path_ref| path_ref.tier == PathTier::Strong)
            .map(|path_ref| path_ref.raw)
    }

    /// 返回文件目标候选。OSC 8 为唯一权威目标；纯文本的 CLI 折行候选
    /// 必须由调用方在后台验证文件存在且唯一，不能直接打开首个候选。
    pub fn preview_path_candidates_at(&self, row: usize, col: usize) -> Vec<String> {
        let mut candidates: Vec<_> = self.preview_path_at(row, col).into_iter().collect();
        if self.hyperlink_at(row, col).is_some() {
            return candidates;
        }
        let Some(cell) = self.rows.get(row).and_then(|cells| cells.get(col)) else {
            return candidates;
        };
        if cell.c.is_whitespace() && !cell.attrs.wide_char_spacer {
            return candidates;
        }

        // ponytail: 无语义目标时只恢复最多 8 个可见行；更长/离屏目标应由 CLI 提供 OSC 8。
        const MAX_ROWS: usize = 8;
        for first in row.saturating_sub(MAX_ROWS - 1)..=row {
            let Some(mut previous) = path_row_token(&self.rows[first], true) else {
                continue;
            };
            if (first == row && !(previous.start..previous.end).contains(&col))
                || !previous.text.contains('/')
                || (previous.text.contains("://") && !previous.text.starts_with("file://"))
            {
                continue;
            }
            let mut path = previous.text.clone();
            for next_row in first + 1..(first + MAX_ROWS).min(self.rows.len()) {
                if is_preview_path_candidate(trim_url_trailing_punctuation(&path)) {
                    break;
                }
                let Some(next) = path_row_token(&self.rows[next_row], false) else {
                    break;
                };
                let previous_cells = &self.rows[next_row - 1];
                let soft = row_soft_wraps(previous_cells);
                let wraps = if soft {
                    previous.end == previous_cells.len() && next.start == 0
                } else {
                    // CLI 在目录/连字符边界换行，或在行末截断长文件名；
                    // 下一片段原本放得下时，不把普通 CRLF 当成显示折行。
                    (previous.text.ends_with(['/', '-', '_', '.'])
                        || previous.end == previous_cells.len())
                        && previous.end + (next.end - next.start) > previous_cells.len()
                        && next.start <= previous.start
                };
                if !wraps
                    || next.text.starts_with(['/', '~'])
                    || next.text.starts_with("./")
                    || next.text.starts_with("../")
                    || next.text.contains("://")
                    || (next_row == row && !(next.start..next.end).contains(&col))
                {
                    break;
                }
                path.push_str(&next.text);
                // 4096 bytes 与常见 Unix PATH_MAX 同量级，限制每次命中的字符串工作量。
                if path.len() > 4096 {
                    break;
                }
                let target = cut_at_fullwidth_punct(trim_url_trailing_punctuation(&path));
                if next_row >= row
                    && is_preview_path_candidate(target)
                    && !candidates.iter().any(|candidate| candidate == target)
                {
                    candidates.push(target.to_string());
                }
                if self.rows[next_row][next.end..]
                    .iter()
                    .any(|cell| !cell.c.is_whitespace())
                {
                    break;
                }
                previous = next;
            }
        }
        candidates
    }
}

struct PathRowToken {
    start: usize,
    end: usize,
    text: String,
}

fn path_row_token(row: &[Cell], last: bool) -> Option<PathRowToken> {
    let is_space = |cell: &Cell| cell.c.is_whitespace() && !cell.attrs.wide_char_spacer;
    let (start, end) = if last {
        let end = row.iter().rposition(|cell| !is_space(cell))? + 1;
        let start = row[..end].iter().rposition(is_space).map_or(0, |i| i + 1);
        let start = fullwidth_segments(row, start, end)
            .first()
            .map_or(start, |&(segment_start, _)| segment_start);
        (start, end)
    } else {
        let start = row.iter().position(|cell| !is_space(cell))?;
        let end = row[start..]
            .iter()
            .position(is_space)
            .map_or(row.len(), |i| start + i);
        (start, end)
    };
    if row[start..end].iter().any(|cell| cell.hyperlink.is_some()) {
        return None;
    }
    let text = row[start..end]
        .iter()
        .filter(|cell| !cell.attrs.wide_char_spacer)
        .map(|cell| cell.c)
        .collect();
    Some(PathRowToken { start, end, text })
}

fn wrapped_terminal_token_at(
    rows: &[Vec<Cell>],
    clicked_row: usize,
    clicked_col: usize,
) -> Option<String> {
    let clicked_cells = rows.get(clicked_row)?;
    if clicked_col >= clicked_cells.len() {
        return None;
    }

    let mut first_row = clicked_row;
    while first_row > 0 && row_soft_wraps(&rows[first_row - 1]) {
        first_row -= 1;
    }
    let mut last_row = clicked_row;
    while last_row + 1 < rows.len() && row_soft_wraps(&rows[last_row]) {
        last_row += 1;
    }

    let clicked_index = rows[first_row..clicked_row]
        .iter()
        .map(Vec::len)
        .sum::<usize>()
        + clicked_col;
    let chars = rows[first_row..=last_row]
        .iter()
        .flat_map(|row| row.iter().map(|cell| cell.c))
        .collect::<Vec<_>>();
    terminal_token_at(&chars, clicked_index)
}

fn row_soft_wraps(row: &[Cell]) -> bool {
    row.last().map(|cell| cell.attrs.soft_wrap).unwrap_or(false)
}

fn terminal_token_at(chars: &[char], clicked_col: usize) -> Option<String> {
    let start = (0..=clicked_col.min(chars.len().saturating_sub(1)))
        .rev()
        .find(|&index| index == 0 || chars[index - 1].is_whitespace())?;
    let end = chars[start..]
        .iter()
        .position(|ch| ch.is_whitespace())
        .map(|offset| start + offset)
        .unwrap_or(chars.len());
    (clicked_col >= start && clicked_col < end).then(|| chars[start..end].iter().collect())
}

fn bare_web_urls(row: &[Cell]) -> Vec<BareWebUrl> {
    let chars = row.iter().map(|cell| cell.c).collect::<Vec<_>>();
    let mut links = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        if starts_web_scheme(&chars[start..]) {
            let end = url_token_end(&chars, start);
            let candidate = chars[start..end].iter().collect::<String>();
            let trimmed = trim_url_trailing_punctuation(&candidate);
            let link_end = start + trimmed.chars().count();
            if is_valid_web_url(trimmed) {
                links.push(BareWebUrl {
                    start,
                    end: link_end,
                    url: trimmed.to_string(),
                });
            }
            start = end.saturating_add(1);
        } else {
            start += 1;
        }
    }
    links
}

fn bare_uri_candidate_at(row: &[Cell], clicked_col: usize) -> bool {
    let chars = row.iter().map(|cell| cell.c).collect::<Vec<_>>();
    let mut start = 0;
    while start < chars.len() {
        if starts_uri_scheme(&chars[start..]) {
            let end = url_token_end(&chars, start);
            if clicked_col >= start && clicked_col < end {
                return true;
            }
            start = end.saturating_add(1);
        } else {
            start += 1;
        }
    }
    false
}

fn url_token_end(chars: &[char], start: usize) -> usize {
    chars[start..]
        .iter()
        .position(|ch| ch.is_whitespace() || matches!(ch, '<' | '>' | '"' | '\'' | '`'))
        .map(|offset| start + offset)
        .unwrap_or(chars.len())
}

fn starts_web_scheme(chars: &[char]) -> bool {
    chars.starts_with(&['h', 't', 't', 'p', ':', '/', '/'])
        || chars.starts_with(&['h', 't', 't', 'p', 's', ':', '/', '/'])
}

fn starts_uri_scheme(chars: &[char]) -> bool {
    let Some(first) = chars.first() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    let mut index = 1;
    while let Some(ch) = chars.get(index) {
        if matches!(ch, ':' | '/' | '\\') {
            break;
        }
        if !ch.is_ascii_alphanumeric() && !matches!(ch, '+' | '-' | '.') {
            return false;
        }
        index += 1;
    }
    chars.get(index..index + 3) == Some(&[':', '/', '/'])
}

fn trim_url_trailing_punctuation(url: &str) -> &str {
    url.trim_end_matches(['.', ',', ';', ':', '!', ')', ']', '}'])
}

fn is_preview_path_candidate(path: &str) -> bool {
    parse_path_ref(path).is_some_and(|path_ref| path_ref.tier == PathTier::Strong)
}

fn is_valid_web_url(url: &str) -> bool {
    let Some(authority) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return false;
    };
    let host = authority.split(['/', '?', '#']).next().unwrap_or_default();
    !host.is_empty()
        && !host.starts_with(':')
        && !host
            .chars()
            .any(|ch| ch.is_whitespace() || matches!(ch, '\\' | '@'))
}
