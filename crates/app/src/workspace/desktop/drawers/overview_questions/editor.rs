use super::AnswerDraft;
use crate::app::OverviewQuestion;
use gpui::{App, AppContext, Context, Entity, Subscription, Window};
use norma::protocol::{QuestionAnswer, QuestionItem, QuestionRef};
use ui::component::input::{InputEvent, InputState};

pub(crate) struct QuestionEditor {
    draft: AnswerDraft,
    inputs: Vec<Entity<InputState>>,
    _subscriptions: Vec<Subscription>,
}

impl QuestionEditor {
    pub(crate) fn new<T: 'static>(
        row: &OverviewQuestion,
        window: &mut Window,
        cx: &mut Context<T>,
    ) -> Self {
        let question = &row.snapshot.question;
        let initial = row
            .suggestion
            .as_ref()
            .map(|s| s.answers.as_slice())
            .unwrap_or_default();
        let draft = AnswerDraft::new(&question.items, Some(question.target.clone()), initial);
        let texts = question
            .items
            .iter()
            .map(|item| {
                initial
                    .iter()
                    .find(|a| a.key == item.key)
                    .and_then(|a| a.text.clone())
                    .unwrap_or_default()
            })
            .collect();
        Self::mount(draft, texts, window, cx)
    }

    pub(crate) fn from_items<T: 'static>(
        items: &[QuestionItem],
        window: &mut Window,
        cx: &mut Context<T>,
    ) -> Self {
        let mut draft = AnswerDraft::new(items, None, &[]);
        for (index, item) in items.iter().enumerate() {
            if item.options.is_empty() {
                draft.custom(index, true);
            }
        }
        Self::mount(draft, vec![String::new(); items.len()], window, cx)
    }

    /// A FocusHandle and InputState's listeners belong to their original Window.
    /// Preserve the incomplete draft while recreating only those window-bound controls.
    pub(crate) fn remount<T: 'static>(self, window: &mut Window, cx: &mut Context<T>) -> Self {
        let texts = self
            .inputs
            .iter()
            .map(|input| input.read(cx).value().to_string())
            .collect();
        Self::mount(self.draft, texts, window, cx)
    }

    fn mount<T: 'static>(
        draft: AnswerDraft,
        texts: Vec<String>,
        window: &mut Window,
        cx: &mut Context<T>,
    ) -> Self {
        let inputs: Vec<_> = draft
            .items
            .iter()
            .zip(texts)
            .map(|(item, text)| {
                cx.new(|cx| {
                    InputState::new(window, cx)
                        .default_value(text)
                        .masked(item.secret)
                        .placeholder(ui::i18n::input_placeholder(
                            "chrome.question_custom_placeholder",
                            window,
                            cx,
                        ))
                })
            })
            .collect();
        let subscriptions = inputs
            .iter()
            .map(|input| {
                cx.subscribe(input, |_, _, event, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                })
            })
            .collect();
        Self {
            draft,
            inputs,
            _subscriptions: subscriptions,
        }
    }

    pub(crate) fn item_count(&self) -> usize {
        self.inputs.len()
    }

    pub(crate) fn target(&self) -> &QuestionRef {
        self.draft
            .target
            .as_ref()
            .expect("native editor has question identity")
    }

    pub(crate) fn answers(&self, cx: &App) -> Vec<QuestionAnswer> {
        self.draft.answers(
            &self
                .inputs
                .iter()
                .map(|input| input.read(cx).value().to_string())
                .collect::<Vec<_>>(),
        )
    }

    pub(crate) fn selected(&self, index: usize, key: &str) -> bool {
        self.draft.selected[index].iter().any(|value| value == key)
    }
    pub(crate) fn custom_selected(&self, index: usize) -> bool {
        self.draft.custom[index]
    }
    pub(crate) fn choose(&mut self, index: usize, key: &str) {
        self.draft.choose(index, key);
    }
    pub(crate) fn toggle_custom(&mut self, index: usize) {
        self.draft.custom(index, !self.draft.custom[index]);
    }
    pub(crate) fn input(&self, index: usize) -> &Entity<InputState> {
        &self.inputs[index]
    }
}
