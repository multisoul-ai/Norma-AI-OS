//! Text preparation and boundary policy for assistant speech.

use std::time::Duration;

/// The maximum additional wait before a short complete sentence is spoken alone.
pub const SHORT_SPEECH_MERGE_WINDOW: Duration = Duration::from_millis(250);

const MIN_SPEECH_SEGMENT_CHARS: usize = 8;
/// Forces a bounded wait when a streamed model does not produce punctuation.
pub const MAX_SPEECH_SEGMENT_CHARS: usize = 60;

/// Converts a display reply into text that is safe and natural to speak.
pub struct SpokenTextNormalizer;

impl SpokenTextNormalizer {
    pub fn normalize(display_text: &str) -> String {
        let mut spoken_lines = Vec::new();
        let mut inside_code_block = false;

        for line in display_text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                if !inside_code_block {
                    spoken_lines.push("详细内容已显示在屏幕上。".to_string());
                }
                inside_code_block = !inside_code_block;
                continue;
            }
            if inside_code_block || trimmed.is_empty() {
                continue;
            }

            let text = trim_display_prefix(trimmed);
            let text = strip_markdown_emphasis_and_code(text);
            let text = replace_urls(&text);
            if !text.trim().is_empty() {
                spoken_lines.push(text);
            }
        }

        spoken_lines.join(" ")
    }
}

/// Accumulates streamed display text and releases speech-sized utterances.
#[derive(Default)]
pub struct UtteranceCommitter {
    pending: String,
    short_sentence_deadline: Option<Duration>,
}

impl UtteranceCommitter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a model text delta and returns every speech segment now safe to synthesize.
    pub fn push_delta(&mut self, delta: &str, now: Duration) -> Vec<String> {
        self.pending.push_str(delta);
        self.drain_complete_segments(now)
    }

    /// Releases a short sentence after its merge window expires.
    pub fn on_idle(&mut self, now: Duration) -> Vec<String> {
        let Some(deadline) = self.short_sentence_deadline else {
            return Vec::new();
        };
        if now < deadline {
            return Vec::new();
        }

        let Some(boundary) = first_strong_boundary(&self.pending) else {
            self.short_sentence_deadline = None;
            return Vec::new();
        };
        self.short_sentence_deadline = None;
        speech_segment_from_display_text(take_through_byte(&mut self.pending, boundary))
            .into_iter()
            .collect()
    }

    /// Flushes all remaining text after the model marks the response complete.
    pub fn finish(&mut self) -> Vec<String> {
        self.short_sentence_deadline = None;
        speech_segment_from_display_text(std::mem::take(&mut self.pending))
            .into_iter()
            .collect()
    }

    fn drain_complete_segments(&mut self, now: Duration) -> Vec<String> {
        let mut committed = Vec::new();

        loop {
            let boundary = first_sufficient_strong_boundary(&self.pending)
                .or_else(|| forced_boundary(&self.pending));
            let Some(boundary) = boundary else {
                break;
            };
            self.short_sentence_deadline = None;
            if let Some(segment) =
                speech_segment_from_display_text(take_through_byte(&mut self.pending, boundary))
            {
                committed.push(segment);
            }
        }

        if first_strong_boundary(&self.pending).is_some() && self.short_sentence_deadline.is_none()
        {
            self.short_sentence_deadline = Some(now + SHORT_SPEECH_MERGE_WINDOW);
        }
        committed
    }
}

fn speech_segment_from_display_text(display_text: String) -> Option<String> {
    let spoken = SpokenTextNormalizer::normalize(&display_text);
    (!spoken.trim().is_empty()).then_some(spoken)
}

fn first_sufficient_strong_boundary(text: &str) -> Option<usize> {
    strong_boundaries(text)
        .into_iter()
        .find_map(|(character_count, boundary)| {
            (character_count >= MIN_SPEECH_SEGMENT_CHARS
                && character_count <= MAX_SPEECH_SEGMENT_CHARS)
                .then_some(boundary)
        })
}

fn first_strong_boundary(text: &str) -> Option<usize> {
    strong_boundaries(text)
        .into_iter()
        .next()
        .map(|(_, boundary)| boundary)
}

fn strong_boundaries(text: &str) -> Vec<(usize, usize)> {
    let mut boundaries = Vec::new();
    let mut character_count = 0;
    for (byte_index, character) in text.char_indices() {
        character_count += 1;
        if matches!(character, '。' | '！' | '？' | '!' | '?') {
            boundaries.push((character_count, byte_index + character.len_utf8()));
        }
    }
    boundaries
}

fn forced_boundary(text: &str) -> Option<usize> {
    let mut character_count = 0;
    let mut last_weak_boundary = None;
    for (byte_index, character) in text.char_indices() {
        character_count += 1;
        if matches!(character, '，' | '；' | '：' | ',' | ';' | ':')
            && character_count >= MIN_SPEECH_SEGMENT_CHARS
            && character_count <= MAX_SPEECH_SEGMENT_CHARS
        {
            last_weak_boundary = Some(byte_index + character.len_utf8());
        }
        if character_count == MAX_SPEECH_SEGMENT_CHARS {
            return Some(last_weak_boundary.unwrap_or(byte_index + character.len_utf8()));
        }
    }
    None
}

fn take_through_byte(text: &mut String, boundary: usize) -> String {
    let remainder = text.split_off(boundary);
    std::mem::replace(text, remainder)
}

fn trim_display_prefix(line: &str) -> &str {
    let without_heading = line.trim_start_matches('#').trim_start();
    without_heading
        .strip_prefix("- ")
        .or_else(|| without_heading.strip_prefix("* "))
        .unwrap_or(without_heading)
}

fn strip_markdown_emphasis_and_code(text: &str) -> String {
    text.replace("**", "")
        .replace("__", "")
        .replace('`', "")
        .replace('*', "")
}

fn replace_urls(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut remaining = text;

    while let Some(start) = find_url_start(remaining) {
        normalized.push_str(&remaining[..start]);
        let url_and_after = &remaining[start..];
        let url_len = url_and_after
            .char_indices()
            .find_map(|(index, character)| character.is_whitespace().then_some(index))
            .unwrap_or(url_and_after.len());
        let (url, after_url) = url_and_after.split_at(url_len);
        let trailing_punctuation = url.chars().last().filter(|character| {
            matches!(character, '。' | '，' | '！' | '？' | '.' | ',' | '!' | '?')
        });
        normalized.push_str("链接已显示在屏幕上");
        if let Some(punctuation) = trailing_punctuation {
            normalized.push(punctuation);
        }
        remaining = after_url;
    }
    normalized.push_str(remaining);
    normalized
}

fn find_url_start(text: &str) -> Option<usize> {
    text.match_indices("https://")
        .map(|(index, _)| index)
        .chain(text.match_indices("http://").map(|(index, _)| index))
        .min()
}

#[cfg(test)]
mod local_tests {
    use super::replace_urls;

    #[test]
    fn test_replace_urls_keeps_sentence_punctuation() {
        let normalized = replace_urls("详情见 https://example.com/report。");

        assert_eq!(
            normalized, "详情见 链接已显示在屏幕上。",
            "URL 末尾句号应保留在口语提示之后"
        );
    }
}
