//! The island's Staff tab: global chat transcript, per-sentence captions and the report button.
use super::{staff, style, style::Tone, view::IslandView};
use gpui::{
    div, prelude::*, px, rgb, AnyElement, Context, Entity, FontWeight, SharedString, Window,
};
use runtime::island::staff::StaffRole;
use runtime::island::IslandEvent;
use ui::component::input::{Input, InputEvent, InputState};
use ui::component::Disableable;
use ui::i18n::text;

/// Redraw cadence while a caption is on screen; sentences last at least 800ms.
const CAPTION_TICK: std::time::Duration = std::time::Duration::from_millis(250);

impl IslandView {
    pub(super) fn staff_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if let Err(reason) = staff::availability(cx) {
            // Without any workspace the brain's state is unknown: the generic enable guide fits.
            let reason = (reason != "island.staff_no_workspace").then_some(reason);
            return self.staff_guide(reason, cx).into_any_element();
        }
        let input = self.staff_input(window, cx);
        let state = staff::state(cx);
        staff::refresh_activity(cx);
        let (lines, caption, notice, report_pending, activity) = {
            let staff = state.read(cx);
            let lines: Vec<_> = staff
                .chat
                .lines()
                .map(|line| (line.id, line.role, line.text.clone()))
                .collect();
            (
                lines,
                staff.caption().map(str::to_owned),
                staff.notice.clone(),
                staff.chat.report_pending(),
                staff.activity.clone(),
            )
        };
        if caption.is_some() {
            self.schedule_caption_tick(cx);
        }
        let mut transcript = div()
            .id("island-staff-transcript")
            .flex()
            .flex_col()
            .gap(px(8.))
            .max_h(px(170.))
            .overflow_y_scroll();
        if lines.is_empty() {
            transcript = transcript.child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(style::MUTED))
                    .child(text("island.staff_empty")),
            );
        }
        for (id, role, body) in lines {
            let mine = role == StaffRole::User;
            transcript = transcript.child(
                div()
                    .id(SharedString::from(format!("island-staff-line-{id}")))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(10.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(if mine { style::MUTED } else { style::STAFF }))
                            .child(text(if mine {
                                "island.staff_you"
                            } else {
                                "island.staff_staff"
                            })),
                    )
                    .child(div().text_size(px(12.5)).child(body)),
            );
        }
        let send = {
            let input = input.clone();
            move |view: &mut IslandView, window: &mut Window, cx: &mut Context<IslandView>| {
                view.submit_staff(&input, window, cx)
            }
        };
        let mut panel = div().flex().flex_col().gap(px(10.)).child(transcript);
        if let Some(caption) = caption {
            panel = panel.child(
                div()
                    .id("island-staff-caption")
                    .px(px(10.))
                    .py(px(6.))
                    .rounded(px(9.))
                    .bg(rgb(0x14121f))
                    .text_size(px(13.))
                    .text_color(rgb(style::FG))
                    .child(caption),
            );
        }
        if let Some(notice) = notice {
            panel = panel.child(
                div()
                    .text_size(px(11.5))
                    .text_color(rgb(style::ATTENTION))
                    .child(notice.to_string()),
            );
        }
        if !activity.is_empty() {
            let mut log = div().flex().flex_col().gap(px(3.)).child(
                div()
                    .text_size(px(10.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(style::FAINT))
                    .child(text("island.staff_activity")),
            );
            for line in activity {
                log = log.child(
                    div()
                        .flex()
                        .gap(px(8.))
                        .text_size(px(11.))
                        .text_color(rgb(style::MUTED))
                        .child(div().flex_none().child(line.label))
                        .child(div().min_w(px(0.)).truncate().child(line.summary)),
                );
            }
            panel = panel.child(log);
        }
        let send_click = send.clone();
        panel
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|_, _, window, cx| {
                                    cx.activate(true);
                                    window.activate_window();
                                }),
                            )
                            .child(
                                Input::new(&input)
                                    .w_full()
                                    .h(px(30.))
                                    .rounded(px(9.))
                                    .text_size(px(12.5))
                                    .bg(rgb(0x070707)),
                            ),
                    )
                    .child(
                        style::action(
                            "island-staff-send",
                            text("island.staff_send"),
                            Tone::Primary,
                            cx,
                        )
                        .on_click(
                            cx.listener(move |view, _, window, cx| send_click(view, window, cx)),
                        ),
                    )
                    .child(
                        style::action(
                            "island-staff-report",
                            text("island.staff_report"),
                            Tone::Neutral,
                            cx,
                        )
                        .disabled(report_pending)
                        .on_click(cx.listener(|view, _, _, cx| view.request_staff_report(cx))),
                    ),
            )
            .into_any_element()
    }

    fn staff_input(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<InputState> {
        if let Some(input) = &self.staff_input {
            return input.clone();
        }
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(ui::i18n::input_placeholder(
                "island.staff_placeholder",
                window,
                cx,
            ))
        });
        self._staff_subscription = Some(cx.subscribe_in(
            &input,
            window,
            move |view, input, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => view.submit_staff(input, window, cx),
                InputEvent::Focus => view.event(IslandEvent::InputFocusChanged(true), cx),
                InputEvent::Blur => view.event(IslandEvent::InputFocusChanged(false), cx),
                _ => {}
            },
        ));
        self.staff_input = Some(input.clone());
        input
    }

    fn submit_staff(
        &mut self,
        input: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let body = input.read(cx).value().to_string();
        if body.trim().is_empty() {
            return;
        }
        input.update(cx, |input, cx| input.set_value(String::new(), window, cx));
        staff::send_message(&body, cx);
        cx.notify();
    }

    fn request_staff_report(&mut self, cx: &mut Context<Self>) {
        let workspaces = self.controller.read(cx).visible_workspaces(cx);
        staff::request_report(&workspaces, cx);
        cx.notify();
    }

    fn schedule_caption_tick(&mut self, cx: &mut Context<Self>) {
        if self.caption_tick.is_some() {
            return;
        }
        self.caption_tick = Some(cx.spawn(async move |view, cx| {
            cx.background_executor().timer(CAPTION_TICK).await;
            let _ = view.update(cx, |view, cx| {
                view.caption_tick = None;
                cx.notify();
            });
        }));
    }
}
