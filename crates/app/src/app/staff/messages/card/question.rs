use super::super::*;
use super::{question_choice::choice, shell};
use gpui::rgba;
use norma::protocol::{validate_question_answers, StaffQuestion, StaffQuestionState};
use ui::component::{input::Input, Disableable, StyledExt};
use ui::i18n::text as t;

pub(in crate::app::staff::messages) struct Question<'a>(pub &'a StaffQuestion);
impl MessageBlock for Question<'_> {
    fn render(
        &self,
        ctx: &BlockCtx,
        _window: &mut Window,
        cx: &mut Context<MultisoulApp>,
    ) -> AnyElement {
        let question = self.0;
        let theme = ctx.app.staff_theme();
        let pending = question.state == StaffQuestionState::Pending;
        let available = question
            .worker
            .as_ref()
            .is_none_or(|target| ctx.app.runtime.native_question_available(target));
        let id = question.id.clone();
        let mut card = shell::card(ctx, true)
            .id(gpui::SharedString::from(format!("question-{id}")))
            .debug_selector(move || format!("staff-question-{id}"))
            .child(
                shell::header()
                    .font_semibold()
                    .text_size(px(tokens::LABEL_TEXT))
                    .child(crate::app::chrome::brand_logo())
                    .child(t(if question.worker.is_some() {
                        "staff.worker_question"
                    } else {
                        "staff.question"
                    }))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_color(rgba(if pending {
                                theme.accent_cyan
                            } else {
                                theme.text_secondary
                            }))
                            .child(t(ctx.app.staff_question_status(question))),
                    ),
            );
        if pending && !available {
            if let Some(target) = &question.worker {
                let node = target.node_id;
                card = card.child(
                    shell::action_button(
                        ctx,
                        gpui::SharedString::from(format!("question-native-{}", question.id)),
                        false,
                        cx,
                    )
                    .label(t("chrome.question_native"))
                    .on_click(cx.listener(move |app, _, window, cx| {
                        app.locate_staff_worker(node, window, cx)
                    })),
                );
            }
        }
        let editor = ctx.app.staff.question_editors.get(&question.id);
        for (index, item) in question.items.iter().enumerate() {
            card = card.child(div().font_medium().child(item.prompt.clone()));
            if pending {
                if let Some(editor) = editor {
                    for (option_index, option) in item.options.iter().enumerate() {
                        card = card.child(choice(
                            ctx,
                            question,
                            index,
                            Some((option_index, option)),
                            editor.selected(index, &option.key),
                            cx,
                        ));
                    }
                    if item.allow_custom {
                        if !item.options.is_empty() {
                            card = card.child(choice(
                                ctx,
                                question,
                                index,
                                None,
                                editor.custom_selected(index),
                                cx,
                            ));
                        }
                        if editor.custom_selected(index) {
                            card = card.child(
                                Input::new(editor.input(index))
                                    .disabled(ctx.app.staff.busy || !available)
                                    .w_full(),
                            );
                        }
                    }
                }
            } else if question.state == StaffQuestionState::Answered {
                let summary = StaffQuestion {
                    items: vec![item.clone()],
                    ..question.clone()
                }
                .answer_text(&question.answers);
                card = card.child(
                    div()
                        .text_size(px(tokens::LABEL_TEXT))
                        .text_color(rgba(theme.text_secondary))
                        .child(
                            summary
                                .strip_prefix(&format!("{}\n", item.prompt))
                                .unwrap_or(&summary)
                                .to_owned(),
                        ),
                );
            }
        }
        if pending {
            let valid = editor.is_some_and(|editor| {
                validate_question_answers(&question.items, &editor.answers(cx)).is_ok()
            });
            let task_id = ctx.task.id.clone();
            let question_id = question.id.clone();
            let submit_id = question.id.clone();
            let skip_task = task_id.clone();
            let skip_question = question_id.clone();
            card = card.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(tokens::SPACE_8))
                    .child(
                        shell::action_button(
                            ctx,
                            gpui::SharedString::from(format!("question-submit-{}", question.id)),
                            true,
                            cx,
                        )
                        .debug_selector(move || format!("staff-question-submit-{submit_id}"))
                        .label(t("staff.question_submit"))
                        .disabled(ctx.app.staff.busy || !available || !valid)
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.answer_staff_question(
                                task_id.clone(),
                                question_id.clone(),
                                false,
                                cx,
                            )
                        })),
                    )
                    .child(
                        shell::action_button(
                            ctx,
                            gpui::SharedString::from(format!("question-skip-{}", question.id)),
                            false,
                            cx,
                        )
                        .label(t("staff.question_skip"))
                        .disabled(ctx.app.staff.busy || !available)
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.answer_staff_question(
                                skip_task.clone(),
                                skip_question.clone(),
                                true,
                                cx,
                            )
                        })),
                    ),
            );
        }
        card.into_any_element()
    }
}
