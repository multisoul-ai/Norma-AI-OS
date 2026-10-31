//! 原题旁的采用、编辑与原卡片入口共享宿主授权和投递服务。
#[cfg(test)]
#[path = "../../../../tests/unit/overview_question_keyboard_tests.rs"]
mod keyboard_tests;
#[cfg(test)]
#[path = "../../../../tests/unit/overview_question_ui_tests.rs"]
mod tests;
use super::super::overview_rows::{OverviewAction, OverviewScreen};
use crate::app::{MultisoulApp, OverviewQuestion};
use gpui::{Context, Window};
use norma::protocol::{PendingQuestion, QuestionAnswer};
mod editor;
mod view;
pub(crate) use editor::QuestionEditor;

#[derive(Clone)]
struct AnswerDraft {
    question: PendingQuestion,
    selected: Vec<Vec<String>>,
    custom: Vec<bool>,
}

impl AnswerDraft {
    fn new(question: &PendingQuestion, initial: &[QuestionAnswer]) -> Self {
        let valid = question.validate_answers(initial).is_ok();
        let answer = |key: &str| {
            valid
                .then(|| initial.iter().find(|a| a.key == key))
                .flatten()
        };
        Self {
            question: question.clone(),
            selected: question
                .items
                .iter()
                .map(|item| {
                    answer(&item.key)
                        .map(|a| a.selected.clone())
                        .unwrap_or_default()
                })
                .collect(),
            custom: question
                .items
                .iter()
                .map(|item| answer(&item.key).is_some_and(|a| a.text.is_some()))
                .collect(),
        }
    }

    fn choose(&mut self, index: usize, key: &str) {
        let Some(item) = self.question.items.get(index) else {
            return;
        };
        if !item.options.iter().any(|option| option.key == key) {
            return;
        }
        let choices = &mut self.selected[index];
        if !item.multiple {
            choices.clear();
            choices.push(key.into());
            self.custom[index] = false;
        } else if choices.iter().any(|choice| choice == key) {
            choices.retain(|choice| choice != key);
        } else {
            choices.push(key.into());
        }
    }

    fn custom(&mut self, index: usize, enabled: bool) {
        let Some(item) = self.question.items.get(index) else {
            return;
        };
        if !item.allow_custom {
            return;
        }
        self.custom[index] = enabled;
        if enabled && !item.multiple {
            self.selected[index].clear();
        }
    }

    fn answers(&self, text: &[String]) -> Vec<QuestionAnswer> {
        self.question
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| QuestionAnswer {
                key: item.key.clone(),
                selected: self.selected[index].clone(),
                text: self.custom[index].then(|| text.get(index).cloned().unwrap_or_default()),
            })
            .collect()
    }
}

pub(super) fn merge_question_counts(screen: &mut OverviewScreen, targets: &[(String, u64)]) {
    for section in &mut screen.sections {
        let before = section.rows.len();
        section.rows.retain(|row| {
            !matches!(&row.action,
            OverviewAction::LocateAgent { area_id, node_id }
            if targets.contains(&(area_id.clone(), *node_id)))
        });
        if let Some(count) = &mut section.count {
            *count = count.saturating_sub(before - section.rows.len());
        }
    }
    if let Some(section) = screen.sections.first_mut() {
        section.count = Some(section.count.unwrap_or(0) + targets.len());
    }
}

impl MultisoulApp {
    fn open_question_editor(
        &mut self,
        row: &OverviewQuestion,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !row.available
            || !self.prepare_overview_question_edit(&row.area_id, &row.snapshot.question.target, cx)
        {
            return;
        }
        let target = &row.snapshot.question.target;
        if self
            .desktop_question_editor
            .as_ref()
            .is_none_or(|editor| editor.target() != target)
        {
            if let Some(editor) = self.desktop_question_editor.take() {
                self.desktop_question_drafts.push(editor);
            }
            self.desktop_question_editor = Some(
                match self
                    .desktop_question_drafts
                    .iter()
                    .position(|editor| editor.target() == target)
                {
                    Some(index) => self.desktop_question_drafts.remove(index),
                    None => QuestionEditor::new(row, window, cx),
                },
            );
        }
        self.desktop_workspace.attention.open =
            Some(super::overview_details::DetailKey::Question {
                area_id: row.area_id.clone(),
                target: Box::new(target.clone()),
            });
        self.desktop_workspace.attention.original_open = false;
        cx.notify();
    }

    fn locate_native_question(
        &mut self,
        row: &OverviewQuestion,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.release_overview_question(&row.area_id, &row.snapshot.question.target, cx) {
            self.desktop_question_editor = None;
            self.desktop_question_drafts
                .retain(|editor| editor.target() != &row.snapshot.question.target);
            self.locate_desktop_agent(
                &row.area_id,
                row.snapshot.question.target.node_id,
                window,
                cx,
            );
        }
    }
}
