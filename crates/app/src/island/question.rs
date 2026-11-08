use super::{
    actions::IslandTarget,
    style,
    view::{button, IslandView},
};
use crate::app::OverviewQuestion;
use gpui::{div, prelude::*, px, rgb, AnyElement, Context};
use ui::{
    component::{checkbox::Checkbox, input::Input, radio::Radio, Disableable},
    i18n::text,
};

impl IslandView {
    pub fn render_question(
        &mut self,
        route: IslandTarget,
        row: &OverviewQuestion,
        id: String,
        context: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let waiting = self.controller.read(cx).pending.contains(&id);
        let editing = self.controller.read(cx).editing.get(&id);
        let mut card = style::card().child(style::card_heading(
            &row.agent,
            &format!("{:?}", row.snapshot.question.target.provider),
            context,
            text("island.question_title"),
            text("island.question_label"),
            style::ATTENTION,
            runtime::island::bloub::Mood::Waiting,
        ));
        if !row.available {
            // The Hook was handed back to the agent's own terminal: nothing can be answered
            // here, so show the question and one clear way to reach the card.
            for item in &row.snapshot.question.items {
                card = card.child(div().text_size(px(13.5)).child(item.prompt.clone()));
            }
            return card
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .child(self.answer_on_card_button(&route, row, &id, true, cx)),
                )
                .into_any_element();
        }
        let Some(editing) = editing else {
            return card.into_any_element();
        };
        let answers = editing.editor.answers(cx);
        let valid = row.snapshot.question.validate_answers(&answers).is_ok();
        let question = row.snapshot.question.target.clone();
        let submit_route = route.clone();
        let mut submit = Some(
            button(
                format!("{id}-submit"),
                text("island.submit_reply"),
                true,
                cx,
            )
            .disabled(waiting || !row.available || !valid)
            .on_click(cx.listener(move |view, _, _, cx| {
                view.controller.update(cx, |state, cx| {
                    state.submit_question(
                        submit_route.clone(),
                        question.clone(),
                        answers.clone(),
                        cx,
                    );
                });
            })),
        );
        for (index, item) in row.snapshot.question.items.iter().enumerate() {
            card = card.child(div().text_size(px(13.5)).child(item.prompt.clone()));
            let mut options = div().flex().flex_col().gap(px(6.));
            for option in &item.options {
                let selected = editing.editor.selected(index, &option.key);
                options = options.child(self.question_choice(
                    &route,
                    row,
                    &id,
                    (index, Some(option.key.clone()), option.label.clone()),
                    (item.multiple, selected),
                    cx,
                ));
            }
            card = card.child(options);
            if item.allow_custom {
                let edit_id = id.clone();
                let mut reply = div().flex().items_center().gap(px(6.)).child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |view, _, window, cx| {
                                view.controller.update(cx, |state, cx| {
                                    if let Some(editing) = state.editing.get(&edit_id) {
                                        editing.prepare(cx);
                                    }
                                });
                                cx.activate(true);
                                window.activate_window();
                            }),
                        )
                        .child(
                            Input::new(editing.editor.input(index))
                                .disabled(waiting || !row.available)
                                .w_full()
                                .h(px(30.))
                                .rounded(px(9.))
                                .text_size(px(12.5))
                                .bg(rgb(0x070707)),
                        ),
                );
                if index + 1 == row.snapshot.question.items.len() {
                    reply = reply.children(submit.take());
                }
                card = card.child(reply);
            }
        }
        card = card.child(
            div()
                .flex()
                .justify_end()
                .gap(px(6.))
                .child(self.answer_on_card_button(&route, row, &id, false, cx))
                .children(submit),
        );
        card.into_any_element()
    }

    fn answer_on_card_button(
        &self,
        route: &IslandTarget,
        row: &OverviewQuestion,
        id: &str,
        primary: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let route = route.clone();
        let target = row.snapshot.question.target.clone();
        button(
            format!("{id}-native"),
            text("island.answer_in_card"),
            primary,
            cx,
        )
        .on_click(cx.listener(move |view, _, _, cx| {
            view.controller.update(cx, |state, cx| {
                state.answer_in_card(route.clone(), target.clone(), cx);
            });
        }))
        .into_any_element()
    }

    fn question_choice(
        &self,
        route: &IslandTarget,
        row: &OverviewQuestion,
        id: &str,
        (index, key, label): (usize, Option<String>, String),
        (multiple, selected): (bool, bool),
        cx: &Context<Self>,
    ) -> AnyElement {
        let selector = gpui::SharedString::from(format!(
            "{id}-{index}-{}",
            key.as_deref().unwrap_or("custom")
        ));
        let route = route.clone();
        let target = row.snapshot.question.target.clone();
        let enabled = row.available && !self.controller.read(cx).pending.contains(id);
        let id = id.to_owned();
        let click = cx.listener(move |view: &mut Self, _: &bool, _, cx| {
            view.controller.update(cx, |state, cx| {
                if let Some(editing) = state
                    .editing
                    .get_mut(&id)
                    .filter(|e| e.route == route && e.editor.target() == &target)
                {
                    if !editing.prepare(cx) {
                        return;
                    }
                    if let Some(key) = &key {
                        editing.editor.choose(index, key);
                    } else {
                        editing.editor.toggle_custom(index);
                    }
                    cx.notify();
                }
            });
            cx.stop_propagation();
        });
        if multiple {
            Checkbox::new(selector)
                .child(label)
                .checked(selected)
                .disabled(!enabled)
                .w_full()
                .px(px(10.))
                .py(px(8.))
                .bg(rgb(0x070707))
                .border_1()
                .border_color(rgb(if selected { style::RUN } else { 0x232323 }))
                .text_size(px(12.5))
                .rounded(px(10.))
                .hover(|s| s.bg(rgb(0x303034)))
                .active(|s| s.bg(rgb(0x404048)))
                .on_click(click)
                .into_any_element()
        } else {
            Radio::new(selector)
                .child(label)
                .checked(selected)
                .disabled(!enabled)
                .w_full()
                .px(px(10.))
                .py(px(8.))
                .bg(rgb(0x070707))
                .border_1()
                .border_color(rgb(if selected { style::RUN } else { 0x232323 }))
                .text_size(px(12.5))
                .rounded(px(10.))
                .hover(|s| s.bg(rgb(0x303034)))
                .active(|s| s.bg(rgb(0x404048)))
                .on_click(click)
                .into_any_element()
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/island_question_submit_tests.rs"]
mod tests;

impl super::controller::IslandController {
    /// Release the intercepted question to its native prompt, then bring the card into view.
    pub fn answer_in_card(
        &mut self,
        route: IslandTarget,
        target: norma::protocol::QuestionRef,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = route.host(cx) else {
            self.message = Some(ui::i18n::LocalizedText::new("island.expired"));
            cx.notify();
            return;
        };
        let node = target.node_id;
        host.update(cx, |host, cx| {
            host.release_overview_question_at(&route.workspace, &route.area, &target, cx)
        });
        self.locate(route, node, cx);
    }

    pub fn submit_question(
        &mut self,
        route: IslandTarget,
        target: norma::protocol::QuestionRef,
        answers: Vec<norma::protocol::QuestionAnswer>,
        cx: &mut Context<Self>,
    ) {
        let id = super::controller::alert_key(route.source, &route.workspace, &route.area, &target);
        if self.pending.contains(&id) {
            return;
        }
        let Some(host) = route.host(cx) else {
            self.message = Some(ui::i18n::LocalizedText::new("island.expired"));
            cx.notify();
            return;
        };
        let work = host.update(cx, |host, cx| {
            host.submit_overview_question_at(
                &route.workspace,
                &route.area,
                target.clone(),
                answers,
                cx,
            )
        });
        self.pending.insert(id.clone());
        self.message = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            work.await;
            // Preparing the durable intent only queues the original Hook response. Its
            // transport may still be writing; keep the button disabled until that finishes.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
            let confirmed = loop {
                let Ok(status) = this.update(cx, |_, cx| {
                    route
                        .session(cx)
                        .and_then(|session| session.read(cx).runtime.question_snapshot(&target))
                }) else {
                    return;
                };
                match status.map(|snapshot| snapshot.state) {
                    Some(norma::protocol::QuestionState::Submitting)
                        if std::time::Instant::now() < deadline =>
                    {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(25))
                            .await;
                    }
                    Some(
                        norma::protocol::QuestionState::Submitted
                        | norma::protocol::QuestionState::Answered,
                    ) => break true,
                    _ => break false,
                }
            };
            let _ = this.update(cx, |this, cx| {
                this.pending.remove(&id);
                let confirmed = confirmed && host.read(cx).question_action_error(&target).is_none();
                if confirmed {
                    this.editing.remove(&id);
                    this.release_question_focus(&id, cx);
                }
                this.message = Some(ui::i18n::LocalizedText::new(if confirmed {
                    "island.reply_submitted"
                } else {
                    "island.unconfirmed"
                }));
                this.action_feedback(id, confirmed, cx);
                host.update(cx, |host, cx| host.note_workspace_activity_changed(cx));
                cx.notify();
            });
        })
        .detach();
    }
}
