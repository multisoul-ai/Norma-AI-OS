//! Pure state for the island's global staff chat: bounded transcript, report requests,
//! per-sentence captions and the bounded cross-workspace overview handed to the brain.
use std::collections::VecDeque;

#[cfg(test)]
#[path = "../../tests/unit/island_staff.rs"]
mod tests;

const MAX_LINES: usize = 40;
const MS_PER_CHAR: u64 = 200;
const MIN_SENTENCE_MS: u64 = 800;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StaffRole {
    User,
    Staff,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StaffLine {
    pub id: u64,
    pub role: StaffRole,
    pub text: String,
}

/// What the host must do with a reply that was just recorded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StaffReply {
    pub id: u64,
    /// The user explicitly asked for a report, so speak even when spoken reports are off.
    pub speak: bool,
}

#[derive(Debug, Default)]
pub struct StaffChat {
    lines: VecDeque<StaffLine>,
    next_id: u64,
    report_pending: bool,
}

impl StaffChat {
    pub fn lines(&self) -> impl Iterator<Item = &StaffLine> {
        self.lines.iter()
    }

    pub fn push_user(&mut self, text: &str) -> Option<u64> {
        self.push(StaffRole::User, text)
    }

    /// Marks that the next reply answers an explicit report request; `false` while one is open.
    pub fn request_report(&mut self) -> bool {
        !std::mem::replace(&mut self.report_pending, true)
    }

    pub fn report_pending(&self) -> bool {
        self.report_pending
    }

    /// Gives up an open request (the brain could not be reached) so the button works again.
    pub fn cancel_report(&mut self) {
        self.report_pending = false;
    }

    pub fn push_reply(&mut self, text: &str) -> Option<StaffReply> {
        let id = self.push(StaffRole::Staff, text)?;
        Some(StaffReply {
            id,
            speak: std::mem::take(&mut self.report_pending),
        })
    }

    fn push(&mut self, role: StaffRole, text: &str) -> Option<u64> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.lines.push_back(StaffLine {
            id,
            role,
            text: text.to_owned(),
        });
        while self.lines.len() > MAX_LINES {
            self.lines.pop_front();
        }
        Some(id)
    }
}

/// Splits on CJK and ASCII sentence terminators and newlines; a `.` between digits stays.
pub fn split_sentences(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut sentences = Vec::new();
    let mut current = String::new();
    let mut flush = |current: &mut String| {
        let sentence = current.trim();
        if !sentence.is_empty() {
            sentences.push(sentence.to_owned());
        }
        current.clear();
    };
    for (index, &ch) in chars.iter().enumerate() {
        if ch == '\n' {
            flush(&mut current);
            continue;
        }
        current.push(ch);
        let decimal = ch == '.'
            && index > 0
            && chars[index - 1].is_ascii_digit()
            && chars.get(index + 1).is_some_and(char::is_ascii_digit);
        let ascii_end = matches!(ch, '.' | '!' | '?')
            && chars.get(index + 1).is_none_or(|next| next.is_whitespace());
        if matches!(ch, '。' | '！' | '？' | '；') || (ascii_end && !decimal) {
            flush(&mut current);
        }
    }
    flush(&mut current);
    sentences
}

/// Index of the sentence being spoken `elapsed_ms` after speech started; `None` once finished.
/// Providers expose no sentence boundaries, so duration is estimated from character counts.
pub fn caption_index(sentences: &[String], elapsed_ms: u64) -> Option<usize> {
    let mut end = 0;
    for (index, sentence) in sentences.iter().enumerate() {
        let weight = (sentence.chars().count() as u64 * MS_PER_CHAR).max(MIN_SENTENCE_MS);
        end += weight;
        if elapsed_ms < end {
            return Some(index);
        }
    }
    None
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverviewEntry {
    pub workspace: String,
    pub project: String,
    pub line: String,
}

/// One line per entry, never exceeding `max_chars`; omitted entries are counted in a last line.
pub fn bounded_overview(entries: &[OverviewEntry], max_chars: usize) -> String {
    let lines: Vec<String> = entries
        .iter()
        .map(|entry| format!("- {}/{}：{}", entry.workspace, entry.project, entry.line))
        .collect();
    let mut kept = 0;
    let mut used = 0;
    for line in &lines {
        let cost = line.chars().count() + usize::from(kept > 0);
        if used + cost > max_chars {
            break;
        }
        used += cost;
        kept += 1;
    }
    let omitted = lines.len() - kept;
    let mut out = lines[..kept].join("\n");
    if omitted > 0 {
        let note = format!("（还有 {omitted} 项未列出）");
        // Make room for the note by dropping whole trailing lines.
        while kept > 0 && out.chars().count() + 1 + note.chars().count() > max_chars {
            kept -= 1;
            out = lines[..kept].join("\n");
        }
        let omitted = lines.len() - kept;
        let note = format!("（还有 {omitted} 项未列出）");
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&note);
    }
    out
}
