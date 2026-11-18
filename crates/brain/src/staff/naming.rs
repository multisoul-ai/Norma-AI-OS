//! 点名只认本轮用户原话里的名册人名或语音别名。
use super::policy::normalize;
use norma::orchestration::OrchestrationProvider;
use norma::protocol::{image_message_parts, TaskMessage};

/// 名册里参与点名匹配的两个称呼。
pub struct NameRef<'a> {
    pub name: &'a str,
    pub speech_alias: Option<&'a str>,
}

/// 上一轮结束后的用户原话。回答选项和图片路径都不算。
pub fn user_words(messages: &[TaskMessage]) -> String {
    let start = messages
        .iter()
        .rposition(|message| match message {
            TaskMessage::Staff(reply) => !reply.is_running(),
            _ => false,
        })
        .map(|index| index + 1)
        .unwrap_or(0);
    let mut words = String::new();
    for message in &messages[start..] {
        let TaskMessage::User { text } = message else {
            continue;
        };
        let (visible, _) = image_message_parts(text);
        if visible.is_empty() {
            continue;
        }
        if !words.is_empty() {
            words.push('\n');
        }
        words.push_str(&visible);
    }
    words
}

/// 原话里是否点了这个名册项。引擎 slug、回退显示名和短于 2 个字符的称呼不参与。
pub fn named_in(words: &str, entry: NameRef<'_>) -> bool {
    mentioned(words, entry.name)
        || entry
            .speech_alias
            .is_some_and(|alias| mentioned(words, alias))
}

fn mentioned(words: &str, candidate: &str) -> bool {
    let candidate = candidate.trim();
    if candidate.chars().count() < 2 || excluded_label(candidate) {
        return false;
    }
    let words = normalize(words);
    if !candidate.is_ascii() {
        return words.contains(candidate);
    }
    let candidate = candidate.to_lowercase();
    let mut from = 0;
    while let Some(offset) = words[from..].find(&candidate) {
        let start = from + offset;
        let end = start + candidate.len();
        let before = words[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
        let after = words[end..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
        if before && after {
            return true;
        }
        from = end;
    }
    false
}

fn excluded_label(candidate: &str) -> bool {
    OrchestrationProvider::ALL.iter().any(|provider| {
        provider.slug().eq_ignore_ascii_case(candidate)
            || provider.display_name().eq_ignore_ascii_case(candidate)
    })
}
