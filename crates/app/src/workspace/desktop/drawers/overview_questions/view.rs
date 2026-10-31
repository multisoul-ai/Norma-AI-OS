use super::*;
use gpui::{div, prelude::*, px, rgba, Div, ElementId, Stateful};
use norma::protocol::QuestionState;
use ui::component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    Disableable, Sizable,
};

impl MultisoulApp {
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
        self.desktop_question_drafts.retain(|editor| {
            rows.iter()
                .any(|row| editor.target() == &row.snapshot.question.target)
        });
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
        let key = super::super::overview_details::DetailKey::Question {
            area_id: row.area_id.clone(),
            target: Box::new(target.clone()),
        };
        let expanded = self.desktop_workspace.attention.is_open(&key);
        let theme = self.desktop_theme();
        let editor = self.render_question_editor(row, &tag, cx);
        let editing = editor.is_some();
        let error = self
            .question_action_error(target)
            .or(row.snapshot.error.as_deref())
            .map(str::to_owned);
        let status = match row.snapshot.state {
            QuestionState::Pending if row.delegated => "chrome.question_delegated",
            QuestionState::Submitting => "chrome.question_submitting",
            QuestionState::Submitted => "chrome.question_submitted",
            QuestionState::Unknown => "chrome.question_unknown",
            QuestionState::Failed => "chrome.question_failed",
            _ if !row.available => "chrome.question_native_only",
            _ => "chrome.question_pending",
        };
        let prompt = row
            .snapshot
            .question
            .items
            .first()
            .map(|item| item.prompt.as_str())
            .unwrap_or_default();
        let title =
            super::super::overview_detail_view::title(prompt, "chrome.brain_question_title");
        let head = super::super::overview_detail_view::header(
            format!("question-toggle-{tag}"),
            key,
            title,
            format!("{} · {}", row.agent, ui::i18n::text(status)),
            "?",
            theme.accent_cyan,
            self,
            cx,
        );
        div()
            .id(ElementId::Name(format!("brain-question-{tag}").into()))
            .debug_selector(move || format!("desktop-brain-question-{index}"))
            .w_full()
            .flex_none()
            .rounded(px(8.))
            .border_1()
            .border_color(rgba(theme.node_stroke))
            .flex()
            .flex_col()
            .text_size(px(12.))
            .text_color(rgba(theme.text_primary))
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
            .child(head)
            .when(expanded, |view| {
                let mut detail = super::super::overview_detail_view::body(theme);
                if !editing {
                    for item in &row.snapshot.question.items {
                        detail = detail.child(
                            div()
                                .text_color(rgba(theme.text_primary))
                                .child(item.prompt.clone()),
                        );
                        for option in &item.options {
                            detail = detail.child(
                                div()
                                    .flex()
                                    .gap(px(6.))
                                    .child(div().flex_none().child("○"))
                                    .child(div().flex_1().min_w(px(0.)).child(
                                        match &option.description {
                                            Some(description) => {
                                                format!("{} — {description}", option.label)
                                            }
                                            None => option.label.clone(),
                                        },
                                    )),
                            );
                        }
                    }
                    if let Some(suggestion) = &row.suggestion {
                        detail = detail
                            .child(div().text_color(rgba(theme.accent_cyan)).child(
                                ui::i18n::format(
                                    "chrome.question_suggestion",
                                    &[(
                                        "answer",
                                        answer_summary(&row.snapshot.question, &suggestion.answers),
                                    )],
                                ),
                            ))
                            .child(div().child(suggestion.reason.clone()));
                    }
                }
                if let Some(editor) = editor {
                    detail = detail.child(editor);
                }
                if error
                    .as_deref()
                    .is_some_and(|e| e != "question_native_takeover")
                {
                    detail = detail.child(
                        div()
                            .text_color(rgba(theme.accent_warm))
                            .child(ui::i18n::text("chrome.question_action_failed")),
                    );
                }
                detail = detail.child(self.question_buttons(row, &tag, editing, cx));
                view.child(detail)
            })
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
                self.question_button(
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
                self.question_button(
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
                self.question_button(
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
            self.question_button(
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

    fn question_button(&self, id: String, key: &'static str, cx: &gpui::App) -> Button {
        let theme = self.desktop_theme();
        Button::new(ElementId::Name(id.clone().into()))
            .debug_selector(move || id)
            .label(ui::i18n::text(key))
            .small()
            .custom(
                ButtonCustomVariant::new(cx)
                    .color(rgba(ui::icons::with_alpha(theme.accent_cyan, 0x18)).into())
                    .foreground(rgba(theme.text_primary).into())
                    .border(rgba(theme.node_stroke).into())
                    .hover(rgba(ui::icons::with_alpha(theme.text_primary, 0x20)).into())
                    .active(rgba(ui::icons::with_alpha(theme.text_primary, 0x35)).into()),
            )
    }
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
