//! 原题旁的采用、编辑与原卡片入口共享宿主授权和投递服务。
use norma::protocol::{PendingQuestion, QuestionAnswer};
mod editor;
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
