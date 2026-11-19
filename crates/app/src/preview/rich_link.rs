//! 富文本点击目标：纯函数识别与改写，不碰 GPUI 与文件系统。
//!
//! 行内代码里的路径在渲染前改写为 `norma-path:` 链接，由统一的链接回调分发；
//! 路径是否存在在点击时由后台线程校验。

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use terminal::parse_path_ref;

const PATH_SCHEME: &str = "norma-path:";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RichTarget {
    /// http(s) 外链：右抽屉浏览器标签。
    Web(String),
    /// 本地路径（可带 `:行:列`）：按终端路径规则解析并在抽屉预览。
    Path(String),
    /// 交给系统（如 mailto）。
    System(String),
    /// 不安全或无法识别，不执行任何动作。
    Reject,
}

pub(crate) fn classify_target(url: &str) -> RichTarget {
    let url = url.trim();
    if url.is_empty() || url.chars().any(char::is_control) {
        return RichTarget::Reject;
    }
    if let Some(encoded) = url.strip_prefix(PATH_SCHEME) {
        return RichTarget::Path(percent_decode(encoded));
    }
    if let Some(path) = url.strip_prefix("file://") {
        let path = path.strip_prefix("localhost").unwrap_or(path);
        return RichTarget::Path(percent_decode(path));
    }
    let Some((scheme, rest)) = url.split_once(':') else {
        return RichTarget::Path(percent_decode(url));
    };
    let is_scheme = scheme.len() > 1
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    // `lib.rs:12` 这类行号后缀不是 scheme。
    if !is_scheme || rest.starts_with(|c: char| c.is_ascii_digit()) {
        return RichTarget::Path(percent_decode(url));
    }
    if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") {
        if rest.starts_with("//") {
            return RichTarget::Web(url.to_owned());
        }
        return RichTarget::Reject;
    }
    if scheme.eq_ignore_ascii_case("mailto") {
        return RichTarget::System(url.to_owned());
    }
    RichTarget::Reject
}

/// 消息文本可能来自模型或提示注入：这些类型交给系统打开会直接执行或挂载，一律不放行。
pub(crate) fn is_unsafe_for_system_open(path: &std::path::Path) -> bool {
    const BLOCKED: &[&str] = &[
        "app", "command", "dmg", "pkg", "mpkg", "sh", "zsh", "bash", "terminal", "workflow",
        "scpt", "scptd", "jar", "exe", "bat", "cmd", "msi", "pl", "py", "rb", "action", "webloc",
        "inetloc", "fileloc", "prefpane", "saver", "plugin", "kext", "xpc", "appex", "osax",
    ];
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| BLOCKED.iter().any(|b| ext.eq_ignore_ascii_case(b)))
}

/// 渲染前改写：行内代码路径 → `[`path`](norma-path:…)`；`![](src)` 套一层同目标链接；裸 URL → `<url>`。
/// 代码块与已有链接/图片内的内容保持不变。
pub(crate) fn linkify_inline_paths(markdown: &str) -> String {
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    // 不参与裸 URL 扫描的区间：代码、代码块、链接、图片。
    let mut opaque: Vec<std::ops::Range<usize>> = Vec::new();
    let mut depth = 0usize;
    for (event, range) in Parser::new_ext(markdown, Options::empty()).into_offset_iter() {
        match event {
            Event::Start(tag @ (Tag::Link { .. } | Tag::Image { .. } | Tag::CodeBlock(_))) => {
                if depth == 0 {
                    opaque.push(range.clone());
                    if let Tag::Image { dest_url, .. } = &tag {
                        edits.push((range.clone(), wrap_image(&markdown[range], dest_url)));
                    }
                }
                depth += 1;
            }
            Event::End(TagEnd::Link | TagEnd::Image | TagEnd::CodeBlock) => {
                depth = depth.saturating_sub(1)
            }
            Event::Html(_) | Event::InlineHtml(_) => opaque.push(range),
            Event::Code(code) if depth == 0 => {
                opaque.push(range.clone());
                if let Some(path_ref) = parse_path_ref(&code).filter(|r| r.raw == *code) {
                    let target = percent_encode(&path_ref.raw);
                    let source = &markdown[range.clone()];
                    edits.push((range, format!("[{source}]({PATH_SCHEME}{target})")));
                }
            }
            _ => {}
        }
    }
    for range in bare_urls(markdown, &opaque) {
        edits.push((range.clone(), format!("<{}>", &markdown[range])));
    }
    edits.sort_by_key(|(range, _)| range.start);
    let mut out = markdown.to_owned();
    for (range, text) in edits.into_iter().rev() {
        out.replace_range(range, &text);
    }
    out
}

/// 纯文本（用户消息）：只让裸 URL 可点，其余 Markdown 语法按字面转义，避免 `*`、`#` 被渲染。
pub(crate) fn linkify_plain_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for range in bare_urls(text, &[]) {
        out.push_str(&escape_markdown(&text[last..range.start]));
        out.push_str(&format!("<{}>", &text[range.clone()]));
        last = range.end;
    }
    out.push_str(&escape_markdown(&text[last..]));
    out
}

fn escape_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c == '\n' {
            out.push_str("\\\n");
            continue;
        }
        if c.is_ascii_punctuation() {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn wrap_image(source: &str, dest: &str) -> String {
    let target = if matches!(classify_target(dest), RichTarget::Web(_)) {
        dest.replace(' ', "%20").replace(')', "%29")
    } else {
        format!("{PATH_SCHEME}{}", percent_encode(dest))
    };
    format!("[{source}]({target})")
}

fn bare_urls(markdown: &str, opaque: &[std::ops::Range<usize>]) -> Vec<std::ops::Range<usize>> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(offset) = markdown[from..].find("http") {
        let start = from + offset;
        from = start + 4;
        let rest = &markdown[start..];
        if !(rest.starts_with("http://") || rest.starts_with("https://")) {
            continue;
        }
        if markdown[..start].ends_with(['<', '(', '"'])
            || markdown[..start].ends_with(|c: char| c.is_alphanumeric())
        {
            continue;
        }
        let end = rest
            .find(|c: char| {
                c.is_whitespace() || !c.is_ascii() || matches!(c, '<' | '>' | '`' | '"' | '[' | ']')
            })
            .unwrap_or(rest.len());
        let url =
            rest[..end].trim_end_matches(['.', ',', ';', ':', '!', '?', ')', '）', '。', '，']);
        let range = start..start + url.len();
        if url.len() > "https://".len()
            && !opaque
                .iter()
                .any(|o| o.start < range.end && range.start < o.end)
        {
            from = range.end;
            found.push(range);
        }
    }
    found
}

fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'-' | b'_' | b'~' | b':') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let hex = |i: usize| bytes.get(i).and_then(|b| (*b as char).to_digit(16));
        match (bytes[index], hex(index + 1), hex(index + 2)) {
            (b'%', Some(hi), Some(lo)) => {
                out.push((hi * 16 + lo) as u8);
                index += 3;
            }
            (byte, ..) => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
#[path = "../../tests/unit/rich_link_tests.rs"]
mod tests;
