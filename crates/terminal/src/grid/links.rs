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

    /// 返回当前工作区内可预览的相对文件路径；拒绝绝对路径与父目录穿越。
    pub fn preview_path_at(&self, row: usize, col: usize) -> Option<String> {
        let token = terminal_token_at(self.rows.get(row)?, col)?;
        let path = trim_url_trailing_punctuation(&token);
        is_safe_preview_path(path).then(|| path.to_string())
    }
}

fn terminal_token_at(row: &[Cell], clicked_col: usize) -> Option<String> {
    let chars = row.iter().map(|cell| cell.c).collect::<Vec<_>>();
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

fn is_safe_preview_path(path: &str) -> bool {
    let extension = path.rsplit_once('.').map(|(_, extension)| extension);
    !path.is_empty()
        && !path.starts_with('/')
        && path.contains('/')
        && !path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        && matches!(
            extension,
            Some("md" | "markdown" | "txt" | "png" | "jpg" | "jpeg")
        )
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
