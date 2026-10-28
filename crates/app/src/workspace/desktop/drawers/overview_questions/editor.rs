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
    pub(super) fn new(
        row: &OverviewQuestion,
        window: &mut Window,
        cx: &mut Context<MultisoulApp>,
    ) -> Self {
        let question = &row.snapshot.question;
        let initial = row
            .suggestion
            .as_ref()
            .map(|s| s.answers.as_slice())
            .unwrap_or_default();
        let draft = AnswerDraft::new(question, initial);
        let inputs: Vec<_> = question
            .items
            .iter()
            .map(|item| {
                let text = initial
                    .iter()
                    .find(|a| a.key == item.key)
                    .and_then(|a| a.text.clone())
                    .unwrap_or_default();
                cx.new(|cx| {
                    InputState::new(window, cx)
                        .default_value(text)
                        .masked(item.secret)
                        .placeholder(ui::i18n::text("chrome.question_custom_placeholder"))
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

    pub(super) fn target(&self) -> &QuestionRef {
        &self.draft.question.target
    }

    pub(super) fn answers(&self, cx: &App) -> Vec<QuestionAnswer> {
        self.draft.answers(
            &self
                .inputs
                .iter()
                .map(|input| input.read(cx).value().to_string())
                .collect::<Vec<_>>(),
        )
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
                    Some(key),
                    option.label.clone(),
                    item.multiple,
                    selected,
                    cx,
                ));
                if let Some(description) = &option.description {
                    group = group.child(
                        div()
                            .pl(px(20.))
                            .text_size(px(11.))
                            .text_color(rgba(0xa9c0d5ff))
                            .child(description.clone()),
                    );
                }
            }
            if item.allow_custom {
                group = group.child(self.question_choice(
                    row,
                    tag,
                    index,
                    None,
                    ui::i18n::text("chrome.question_custom").into(),
                    item.multiple,
                    editor.draft.custom[index],
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
        key: Option<String>,
        label: String,
        multiple: bool,
        selected: bool,
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
        let color = rgba(if row.available {
            0xeef6ffff
        } else {
            0x8ea8bfff
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
                .hover(|s| s.bg(rgba(0x58adff2e)))
                .active(|s| s.bg(rgba(0x58adff55)))
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
                .hover(|s| s.bg(rgba(0x58adff2e)))
                .active(|s| s.bg(rgba(0x58adff55)))
                .on_click(click)
                .into_any_element()
        }
    }
}
