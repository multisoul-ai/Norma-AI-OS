use super::*;
use gpui::{div, prelude::*, px, rgba, Div, Window};
use ui::component::{
    button::{Button, ButtonVariants},
    input::{Input, InputEvent},
    Disableable, IconName, Sizable,
};
use ui::i18n::text as t;

impl MultisoulApp {
    pub(super) fn ensure_staff_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            self.staff.collapsed.insert("Done".into());
            self.staff.search = Some(search);
        }
    }

    pub(super) fn staff_composer(
        &self,
        task: Option<&Task>,
        project_name: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        let input = self.staff.input.as_ref().expect("staff input initialized");
        let theme = self.theme;
        div().px(px(28.)).pt(px(8.)).flex_none().child(
            div()
                .max_w(px(760.))
                .mx_auto()
                .flex()
                .flex_col()
                .child(
                    div()
                        .id("staff-composer")
                        .debug_selector(|| "staff-composer".into())
                        .border_1()
                        .border_color(rgba(self.staff_line()))
                        .rounded(px(16.))
                        .bg(rgba(ui::icons::with_alpha(theme.text_primary, 9)))
                        .px(px(16.))
                        .py(px(10.))
                        .capture_action(cx.listener(
                            |app, action: &ui::component::input::Enter, window, cx| {
                                if action.secondary {
                                    app.send_staff_message(window, cx);
                                    cx.stop_propagation();
                                }
                            },
                        ))
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgba(theme.text_secondary))
                                .child(ui::i18n::format(
                                    "staff.send_to_project",
                                    &[("project", project_name.to_owned())],
                                )),
                        )
                        .child(
                            div()
                                .flex()
                                .items_end()
                                .gap(px(10.))
                                .mt(px(6.))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .min_h(px(34.))
                                        .max_h(px(120.))
                                        .child(
                                            Input::new(input)
                                                .appearance(false)
                                                .text_size(px(14.))
                                                .disabled(self.staff.busy),
                                        ),
                                )
                                .child(
                                    Button::new("staff-send")
                                        .debug_selector(|| "staff-send".into())
                                        .primary()
                                        .with_size(px(34.))
                                        .rounded_full()
                                        .icon(IconName::ArrowUp)
                                        .tooltip(t("staff.send"))
                                        .disabled(
                                            self.staff.busy
                                                || input.read(cx).value().trim().is_empty(),
                                        )
                                        .on_click(cx.listener(Self::send_staff_message_click)),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .py(px(10.))
                        .text_size(px(11.))
                        .text_color(rgba(theme.text_tertiary))
                        .child(
                            if task
                                .is_some_and(|task| task.phase == norma::protocol::TaskPhase::Done)
                            {
                                t("staff.reopen_hint")
                            } else {
                                t("staff.composer_hint")
                            },
                        )
                        .child(t("staff.send_shortcut")),
                ),
        )
    }
}
