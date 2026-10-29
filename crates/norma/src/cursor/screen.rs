//! A pure one-step planner for a complete live Cursor question frame, never scrollback.
use super::questions::CursorQuestion;
use crate::protocol::{validate_question_answers, QuestionAnswer};

const FOOTER: &str = "↑/↓ option · ←/→ question · Space select · Enter next/submit · Esc to skip";
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CursorQuestionStep {
    Up,
    Down,
    Space,
    Enter,
    Text(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorQuestionFrame {
    question: usize,
    multiple: bool,
    focus: usize,
    selected: Vec<bool>,
    custom: String,
}
impl CursorQuestionFrame {
    /// Caller supplies only the current terminal viewport and a separately verified live call.
    pub fn parse(question: &CursorQuestion, rows: &[String]) -> Option<Self> {
        if rows.len() > 120 || rows.iter().any(|row| row.chars().count() > 512) {
            return None;
        }
        let rows = rows
            .iter()
            .map(|row| {
                row.trim()
                    .trim_start_matches('│')
                    .trim_end_matches('│')
                    .trim()
            })
            .collect::<Vec<_>>();
        let markers = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.starts_with("Question "))
            .collect::<Vec<_>>();
        if markers.len() != 1 {
            return None;
        }
        let (start, marker) = markers[0];
        let (number, total) = marker.strip_prefix("Question ")?.split_once(" of ")?;
        let (number, total) = (number.parse::<usize>().ok()?, total.parse::<usize>().ok()?);
        if total != question.items().len() || number == 0 || number > total {
            return None;
        }
        let item = &question.items()[number - 1];
        let end = rows
            .iter()
            .enumerate()
            .skip(start + 1)
            .find(|(_, row)| **row == FOOTER)?
            .0;
        let body = rows[start + 1..end]
            .iter()
            .copied()
            .filter(|row| !row.is_empty())
            .collect::<Vec<_>>();
        if body.len() != item.options.len() + 2 {
            return None;
        }
        let prompt = format!(
            "{number}. {}{}",
            item.prompt,
            if item.multiple { " (multi-select)" } else { "" }
        );
        if body[0] != prompt {
            return None;
        }
        let mut selected = Vec::new();
        let mut focus = None;
        let mut custom = String::new();
        for (index, line) in body[1..].iter().enumerate() {
            let (focused, line) = if let Some(line) = line.strip_prefix("› ") {
                (true, line)
            } else {
                (false, *line)
            };
            if focused && focus.replace(index).is_some() {
                return None;
            }
            let (checked, label) = if let Some(label) = line.strip_prefix("[x] ") {
                (true, label)
            } else {
                (false, line.strip_prefix("[ ] ")?)
            };
            if let Some(option) = item.options.get(index) {
                if label != option.label {
                    return None;
                }
            } else {
                let text = label.strip_prefix("Other:")?.trim();
                if text == "(type to answer)" && checked {
                    return None;
                }
                if text != "(type to answer)" {
                    custom = text.into();
                }
                if !checked && !custom.is_empty() {
                    return None;
                }
            }
            selected.push(checked);
        }
        if !item.multiple && selected.iter().filter(|selected| **selected).count() > 1 {
            return None;
        }
        Some(Self {
            question: number - 1,
            multiple: item.multiple,
            focus: focus?,
            selected,
            custom,
        })
    }
    pub fn question_index(&self) -> usize {
        self.question
    }
    pub fn next_step(
        &self,
        question: &CursorQuestion,
        answer: &QuestionAnswer,
    ) -> Result<CursorQuestionStep, &'static str> {
        let item = question
            .items()
            .get(self.question)
            .ok_or("cursor_frame_changed")?;
        validate_question_answers(std::slice::from_ref(item), std::slice::from_ref(answer))?;
        let other = item.options.len();
        if let Some(text) = &answer.text {
            if self.selected[..other].iter().any(|selected| *selected) {
                return Err("cursor_native_draft_changed");
            }
            if !answer.selected.is_empty()
                || text.trim() != text
                || text.chars().any(char::is_control)
            {
                return Err("cursor_custom_unsupported");
            }
            if !self.custom.is_empty() && self.custom != *text {
                return Err("cursor_native_draft_changed");
            }
            if self.focus != other {
                return self.move_to(other);
            }
            if self.custom.is_empty() {
                return Ok(CursorQuestionStep::Text(text.clone()));
            }
            if self.selected[other] {
                return Ok(CursorQuestionStep::Enter);
            }
            return Err("cursor_frame_changed");
        }
        if self.selected[other] || !self.custom.is_empty() {
            return Err("cursor_native_draft_changed");
        }
        // Single-choice Space replaces the old choice. Multi-select requires every mismatch fixed.
        let target = if item.multiple {
            item.options
                .iter()
                .enumerate()
                .find(|(index, option)| {
                    self.selected[*index] != answer.selected.contains(&option.key)
                })
                .map(|(index, _)| index)
        } else {
            item.options
                .iter()
                .position(|option| answer.selected.contains(&option.key))
                .filter(|index| !self.selected[*index])
        };
        match target {
            Some(index) if self.focus == index => Ok(CursorQuestionStep::Space),
            Some(index) => self.move_to(index),
            // Native Enter selects the focused option before submitting. Keep focus on an
            // intended selection even after deselecting an unwanted multi-choice option.
            None if self.selected.get(self.focus) == Some(&true) => Ok(CursorQuestionStep::Enter),
            None => self.move_to(
                self.selected
                    .iter()
                    .position(|selected| *selected)
                    .ok_or("cursor_frame_changed")?,
            ),
        }
    }
    /// A viewport resize or spinner redraw is not an acknowledgement of a question key.
    pub fn confirms_step(&self, step: &CursorQuestionStep, next: &Self) -> bool {
        if matches!(step, CursorQuestionStep::Enter) {
            return next.question == self.question + 1 && next.focus == 0;
        }
        let mut expected = self.clone();
        match step {
            CursorQuestionStep::Up if self.focus > 0 => expected.focus -= 1,
            CursorQuestionStep::Down if self.focus + 1 < self.selected.len() => expected.focus += 1,
            CursorQuestionStep::Space if self.focus + 1 < self.selected.len() => {
                if self.multiple {
                    expected.selected[self.focus] = !expected.selected[self.focus];
                } else {
                    expected.selected.fill(false);
                    expected.selected[self.focus] = true;
                }
            }
            CursorQuestionStep::Text(text)
                if self.focus + 1 == self.selected.len() && self.custom.is_empty() =>
            {
                expected.custom = text.clone();
                expected.selected[self.focus] = true;
            }
            _ => return false,
        }
        expected == *next && self != next
    }

    fn move_to(&self, index: usize) -> Result<CursorQuestionStep, &'static str> {
        if self.focus == self.selected.len() - 1 {
            return Err("cursor_custom_editor_focused");
        }
        Ok(if index < self.focus {
            CursorQuestionStep::Up
        } else {
            CursorQuestionStep::Down
        })
    }
}
