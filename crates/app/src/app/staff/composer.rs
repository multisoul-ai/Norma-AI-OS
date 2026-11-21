use super::images::staff_can_send;
use super::*;
use gpui::{div, prelude::*, px, rgba, Div, Focusable, KeyDownEvent, Window};
use ui::component::{
    button::{Button, ButtonVariants},
    input::{Input, InputEvent},
    Disableable, IconName, Sizable,
};
use ui::i18n::text as t;

impl MultisoulApp {
    pub(super) fn ensure_staff_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_proposal_model_lists(cx);
        if self.staff.input.is_none() {
            let input = cx.new(|cx| {
                InputState::new(window, cx).auto_grow(1, 5).placeholder(
                    ui::i18n::input_placeholder("staff.message_placeholder", window, cx),
                )
            });
            cx.subscribe_in(&input, window, |app, _, _: &InputEvent, _, cx| {
                app.notify_staff(cx)
            })
            .detach();
            self.staff.input = Some(input);
        }
        if self.staff.search.is_none() {
            let search = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(ui::i18n::input_placeholder("staff.search", window, cx))
                    .validate(|value, _| !value.contains(['\n', '\r']))
            });
            cx.subscribe(&search, |app, search, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    app.staff.query = search.read(cx).value().to_string();
                    app.notify_staff(cx);
                }
            })
            .detach();
            self.staff.search = Some(search);
        }
    }

    pub(super) fn staff_composer(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let input = self.staff.input.as_ref().expect("staff input initialized");
        let theme = self.staff_theme();
        let running_task = self
            .staff
            .selected
            .as_ref()
            .and_then(|id| self.staff.tasks.get(id))
            .filter(|task| task.has_running_reply());
        let stop = running_task.map(|task| {
            let input_focus = input.focus_handle(cx);
            let (id, round) = (task.id.clone(), task.round);
            Button::new("staff-stop")
                .debug_selector(|| "staff-stop".into())
                .custom(self.staff_neutral_style(cx))
                .with_size(px(34.))
                .size(px(34.))
                .rounded_full()
                .child(
                    div()
                        .size(px(11.))
                        .rounded(px(2.))
                        .bg(rgba(theme.text_primary)),
                )
                .tooltip(t("staff.stop_generation"))
                .on_click(cx.listener(move |app, _, window, _| {
                    app.stop_staff_turn(&id, round);
                    input_focus.focus(window);
                }))
        });
        // 底部留白：原来提示行占的位置，输入框不贴着面板底边。
        div().px(px(20.)).pt(px(8.)).pb(px(24.)).flex_none().child(
            div().max_w(px(760.)).mx_auto().flex().flex_col().child(
                div()
                    .id("staff-composer")
                    .debug_selector(|| "staff-composer".into())
                    .border_1()
                    .border_color(rgba(if input.focus_handle(cx).is_focused(window) {
                        theme.accent_cyan
                    } else {
                        self.staff_line()
                    }))
                    .rounded(px(22.))
                    .bg(rgba(theme.panel_high))
                    .pl(px(18.))
                    .pr(px(8.))
                    .py(px(8.))
                    .flex()
                    .flex_col()
                    .when(!self.staff.images.is_empty(), |composer| {
                        composer.child(self.staff_image_chips(cx))
                    })
                    .capture_action(cx.listener(
                        |app, _: &ui::component::input::Enter, window, cx| {
                            app.send_staff_message(window, cx);
                            cx.stop_propagation();
                        },
                    ))
                    .capture_action(cx.listener(|app, _: &ui::component::input::Paste, _, cx| {
                        if app.paste_staff_clipboard_image(cx) {
                            cx.stop_propagation();
                        }
                    }))
                    .capture_key_down(cx.listener(|app, event: &KeyDownEvent, window, cx| {
                        let keystroke = &event.keystroke;
                        if keystroke.key == "enter" && keystroke.modifiers.shift {
                            if let Some(input) = app.staff.input.clone() {
                                input.update(cx, |input, cx| input.insert("\n", window, cx));
                            }
                            cx.stop_propagation();
                        }
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(
                                div().w_full().min_h(px(34.)).max_h(px(120.)).child(
                                    Input::new(input)
                                        .appearance(false)
                                        .text_size(px(14.))
                                        .disabled(self.staff.busy),
                                ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        Button::new("staff-attach-image")
                                            .debug_selector(|| "staff-attach-image".into())
                                            .custom(self.staff_image_attach_style(cx))
                                            .with_size(px(32.))
                                            .rounded_full()
                                            .icon(IconName::Plus)
                                            .tooltip(t("staff.attach_image"))
                                            .disabled(
                                                self.staff.busy || self.staff.image_uploads > 0,
                                            )
                                            .on_click(cx.listener(|app, _, _, cx| {
                                                app.pick_staff_image(cx)
                                            })),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.))
                                            .children(stop)
                                            .child(
                                                Button::new("staff-send")
                                                    .debug_selector(|| "staff-send".into())
                                                    .custom(self.staff_primary_style(cx))
                                                    .with_size(px(34.))
                                                    .rounded_full()
                                                    .icon(IconName::ArrowUp)
                                                    .tooltip(t("staff.send"))
                                                    .disabled(!staff_can_send(
                                                        self.staff.busy,
                                                        self.staff.image_uploads > 0,
                                                        &input.read(cx).value(),
                                                        self.staff.images.len(),
                                                    ))
                                                    .on_click(
                                                        cx.listener(Self::send_staff_message_click),
                                                    ),
                                            ),
                                    ),
                            ),
                    ),
            ),
        )
    }
}
