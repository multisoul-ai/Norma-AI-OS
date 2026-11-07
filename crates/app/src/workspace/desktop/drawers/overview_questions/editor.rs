use super::*;
use gpui::{div, prelude::*, px, rgba, AnyElement, App, ElementId, Entity, Subscription};
use norma::protocol::QuestionRef;
use ui::component::{
    checkbox::Checkbox,
    input::{Input, InputEvent, InputState},
    radio::Radio,
    Disableable,
};

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
        let draft = AnswerDraft::new(question, initial);
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
            .question
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
        &self.draft.question.target
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

impl MultisoulApp {
    pub(super) fn render_question_editor(
        &self,
        row: &OverviewQuestion,
        tag: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let editor = self.desktop_question_editor.as_ref()?;
        if editor.target() != &row.snapshot.question.target {
            return None;
        }
        let enabled = row.available;
        let theme = self.desktop_theme();
        let mut body = div().flex().flex_col().gap(px(8.));
        for (index, item) in editor.draft.question.items.iter().enumerate() {
            let mut group = div()
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(item.prompt.clone());
            for option in &item.options {
                let key = option.key.clone();
                let selected = editor.draft.selected[index].contains(&key);
                group = group.child(self.question_choice(
                    row,
                    tag,
                    index,
                    (Some(key), option.label.clone()),
                    (item.multiple, selected),
                    cx,
                ));
                if let Some(description) = &option.description {
                    group = group.child(
                        div()
                            .pl(px(20.))
                            .text_size(px(11.))
                            .text_color(rgba(theme.text_secondary))
                            .child(description.clone()),
                    );
                }
            }
            if item.allow_custom {
                group = group.child(self.question_choice(
                    row,
                    tag,
                    index,
                    (None, ui::i18n::text("chrome.question_custom").into()),
                    (item.multiple, editor.draft.custom[index]),
                    cx,
                ));
                if editor.draft.custom[index] {
                    group = group.child(
                        Input::new(&editor.inputs[index])
                            .disabled(!enabled)
                            .w_full(),
                    );
                }
            }
            body = body.child(group);
        }
        Some(
            body.id(ElementId::Name(format!("question-editor-{tag}").into()))
                .into_any_element(),
        )
    }

    fn question_choice(
        &self,
        row: &OverviewQuestion,
        tag: &str,
        index: usize,
        (key, label): (Option<String>, String),
        (multiple, selected): (bool, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selector = format!(
            "question-option-{tag}-{index}-{}",
            key.as_deref().unwrap_or("custom")
        );
        let id = ElementId::Name(
            format!(
                "question-option-{tag}-{index}-{}",
                key.as_deref().unwrap_or("custom")
            )
            .into(),
        );
        let target = row.snapshot.question.target.clone();
        let click = cx.listener(move |app: &mut Self, _: &bool, _, cx| {
            if let Some(editor) = app
                .desktop_question_editor
                .as_mut()
                .filter(|e| e.target() == &target)
            {
                if let Some(key) = &key {
                    editor.draft.choose(index, key);
                } else {
                    editor.draft.custom(index, !selected);
                }
                cx.notify();
            }
            cx.stop_propagation();
        });
        let theme = self.desktop_theme();
        let color = rgba(if row.available {
            theme.text_primary
        } else {
            theme.text_tertiary
        });
        if multiple {
            Checkbox::new(id)
                .debug_selector(move || selector)
                .child(div().text_color(color).child(label))
                .checked(selected)
                .disabled(!row.available)
                .text_color(color)
                .p(px(6.))
                .rounded(px(6.))
                .hover(|s| s.bg(rgba(ui::icons::with_alpha(theme.text_primary, 0x18))))
                .active(|s| s.bg(rgba(ui::icons::with_alpha(theme.text_primary, 0x30))))
                .on_click(click)
                .into_any_element()
        } else {
            Radio::new(id)
                .debug_selector(move || selector)
                .child(label)
                .checked(selected)
                .disabled(!row.available)
                .text_color(color)
                .p(px(6.))
                .rounded(px(6.))
                .hover(|s| s.bg(rgba(ui::icons::with_alpha(theme.text_primary, 0x18))))
                .active(|s| s.bg(rgba(ui::icons::with_alpha(theme.text_primary, 0x30))))
                .on_click(click)
                .into_any_element()
        }
    }
}

#[cfg(test)]
#[path = "../../../../../tests/unit/island_question_mount_tests.rs"]
mod island_mount_tests;
