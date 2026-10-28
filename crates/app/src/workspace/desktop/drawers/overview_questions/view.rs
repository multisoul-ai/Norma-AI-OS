use super::*;
use gpui::{div, prelude::*, px, rgba, Div, ElementId, Stateful};
use norma::protocol::QuestionState;
use ui::component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    Disableable, Sizable, StyledExt,
};

impl MultisoulApp {
    pub(in crate::workspace::desktop::drawers) fn question_heights(&self) -> Vec<f32> {
        self.brain_overview()
            .questions
            .iter()
            .map(|row| {
                let editing = self
                    .desktop_question_editor
                    .as_ref()
                    .is_some_and(|e| e.target() == &row.snapshot.question.target);
                if editing {
                    260.
                } else {
                    180.
                }
            })
            .collect()
    }

    pub(in crate::workspace::desktop::drawers) fn render_question_rows(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Vec<Stateful<Div>> {
        let rows = self.brain_overview().questions.clone();
        if self.desktop_question_editor.as_ref().is_some_and(|editor| {
            !rows
                .iter()
                .any(|row| &row.snapshot.question.target == editor.target())
        }) {
            self.desktop_question_editor = None;
        }
        rows.iter()
            .enumerate()
            .map(|(index, row)| self.render_question_row(row, index, cx))
            .collect()
    }

    pub(super) fn render_question_row(
        &mut self,
        row: &OverviewQuestion,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let target = &row.snapshot.question.target;
        let tag = format!("{}-{}-{index}", row.area_id, target.node_id);
        let editor = self.render_question_editor(row, &tag, cx);
        let editing = editor.is_some();
        let error = self
            .question_action_error(target)
            .or(row.snapshot.error.as_deref());
        let status = match row.snapshot.state {
            QuestionState::Pending if row.delegated => "chrome.question_delegated",
            QuestionState::Submitting => "chrome.question_submitting",
            QuestionState::Submitted => "chrome.question_submitted",
            QuestionState::Unknown => "chrome.question_unknown",
            QuestionState::Failed => "chrome.question_failed",
            _ if !row.available => "chrome.question_native_only",
            _ => "chrome.question_pending",
        };
        div()
            .id(ElementId::Name(format!("brain-question-{tag}").into()))
            .debug_selector(move || format!("desktop-brain-question-{index}"))
            // Native controls activate once on key-up; keep key-down off overview navigation.
            .on_key_down(|event, _, cx| {
                if !event.keystroke.modifiers.modified()
                    && matches!(
                        event.keystroke.key.as_str(),
                        "enter" | "up" | "down" | "home" | "end"
                    )
                {
                    cx.stop_propagation();
                }
            })
            .w_full()
            .flex_none()
            .p(px(8.))
            .rounded(px(9.))
            .border_1()
            .border_color(rgba(0x83c4ff40))
            .flex()
            .flex_col()
            .gap(px(6.))
            .text_size(px(12.))
            .text_color(rgba(0xeef6ffff))
            .child(div().font_semibold().child(format!(
                "{} · {}",
                row.agent,
                ui::i18n::text(status)
            )))
            .when(!editing, |view| {
                view.children(row.snapshot.question.items.iter().map(|item| {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .child(item.prompt.clone())
                        .child(
                            div().text_color(rgba(0xa9c0d5ff)).child(
                                item.options
                                    .iter()
                                    .map(|o| match &o.description {
                                        Some(description) => {
                                            format!("{} — {}", o.label, description)
                                        }
                                        None => o.label.clone(),
                                    })
                                    .collect::<Vec<_>>()
                                    .join(" · "),
                            ),
                        )
                }))
            })
            .when_some(
                row.suggestion.as_ref().filter(|_| !editing),
                |view, suggestion| {
                    view.child(div().text_color(rgba(0xb9dcffff)).child(ui::i18n::format(
                        "chrome.question_suggestion",
                        &[(
                            "answer",
                            answer_summary(&row.snapshot.question, &suggestion.answers),
                        )],
                    )))
                    .child(
                        div()
                            .text_color(rgba(0xa9c0d5ff))
                            .child(suggestion.reason.clone()),
                    )
                },
            )
            .when_some(editor, |view, editor| view.child(editor))
            .when(
                error.is_some() && error != Some("question_native_takeover"),
                |view| {
                    view.child(
                        div()
                            .text_color(rgba(0xf6dfaaff))
                            .child(ui::i18n::text("chrome.question_action_failed")),
                    )
                },
            )
            .child(self.question_buttons(row, &tag, editing, cx))
    }

    fn question_buttons(
        &mut self,
        row: &OverviewQuestion,
        tag: &str,
        editing: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut buttons = div().flex().flex_wrap().gap(px(4.));
        let answers = if editing {
            self.desktop_question_editor
                .as_ref()
                .map(|editor| editor.answers(cx))
        } else {
            row.suggestion
                .as_ref()
                .map(|suggestion| suggestion.answers.clone())
        };
        if let Some(answers) = answers {
            let available =
                row.available && row.snapshot.question.validate_answers(&answers).is_ok();
            let area = row.area_id.clone();
            let target = row.snapshot.question.target.clone();
            buttons = buttons.child(
                question_button(
                    format!("question-submit-{tag}"),
                    if editing {
                        "chrome.question_send"
                    } else {
                        "chrome.question_adopt"
                    },
                    cx,
                )
                .disabled(!available)
                .on_click(cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    app.submit_overview_question(&area, target.clone(), answers.clone(), cx)
                        .detach();
                })),
            );
        }
        if !editing {
            let edit = row.clone();
            buttons = buttons.child(
                question_button(
                    format!("question-edit-{tag}"),
                    if row.suggestion.is_some() {
                        "chrome.question_edit"
                    } else {
                        "chrome.question_fill"
                    },
                    cx,
                )
                .disabled(!row.available)
                .on_click(cx.listener(move |app, _, window, cx| {
                    cx.stop_propagation();
                    app.open_question_editor(&edit, window, cx);
                })),
            );
        }
        let locate = row.clone();
        if row.can_delegate && !editing {
            let area = row.area_id.clone();
            let target = row.snapshot.question.target.clone();
            buttons = buttons.child(
                question_button(
                    format!("question-delegate-{tag}"),
                    "chrome.question_delegate",
                    cx,
                )
                .disabled(!row.available || row.delegated)
                .on_click(cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    app.delegate_overview_question(&area, &target, cx);
                })),
            );
        }
        buttons.child(
            question_button(
                format!("question-native-{tag}"),
                "chrome.question_native",
                cx,
            )
            .on_click(cx.listener(move |app, _, window, cx| {
                cx.stop_propagation();
                app.locate_native_question(&locate, window, cx);
            })),
        )
    }
}

fn question_button(id: String, key: &'static str, cx: &gpui::App) -> Button {
    Button::new(ElementId::Name(id.clone().into()))
        .debug_selector(move || id)
        .label(ui::i18n::text(key))
        .small()
        .custom(
            ButtonCustomVariant::new(cx)
                .color(rgba(0x58adff26).into())
                .foreground(rgba(0xeef6ffff).into())
                .border(rgba(0x83c4ff55).into())
                .hover(rgba(0x58adff40).into())
                .active(rgba(0x58adff59).into()),
        )
}

fn answer_summary(question: &PendingQuestion, answers: &[QuestionAnswer]) -> String {
    question
        .items
        .iter()
        .filter_map(|item| {
            let answer = answers.iter().find(|answer| answer.key == item.key)?;
            if item.secret {
                return Some("••••".into());
            }
            let mut labels: Vec<_> = answer
                .selected
                .iter()
                .filter_map(|key| {
                    item.options
                        .iter()
                        .find(|option| &option.key == key)
                        .map(|option| option.label.clone())
                })
                .collect();
            labels.extend(answer.text.iter().cloned());
            Some(labels.join(", "))
        })
        .collect::<Vec<_>>()
        .join("; ")
}
