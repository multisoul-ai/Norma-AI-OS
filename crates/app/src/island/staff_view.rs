//! The island's Staff tab, laid out like the approved prototype: orb header with the spoken
//! report pill, summary card, activity timeline, suggestion chips and a message pill.
use super::{staff, style, style::Tone, view::IslandView};
use gpui::{
    div, linear_color_stop, linear_gradient, prelude::*, px, rgb, rgba, AnyElement, Context,
    Entity, FontWeight, SharedString, Window,
};
use runtime::island::staff::{ago, Ago, StaffRole};
use runtime::island::IslandEvent;
use ui::component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    input::{Input, InputEvent, InputState},
    Disableable, Icon, IconName,
};
use ui::i18n::{format, text};

/// Redraw cadence while a caption is on screen; sentences last at least 800ms.
const CAPTION_TICK: std::time::Duration = std::time::Duration::from_millis(250);
const CARD_BG: u32 = 0x151515;
const CHIPS: [(&str, &str); 3] = [
    ("island-chip-stuck", "island.staff_chip_stuck"),
    ("island-chip-needs", "island.staff_chip_needs"),
    ("island-chip-done", "island.staff_chip_done"),
];

fn ago_label(elapsed_ms: u64) -> String {
    match ago(elapsed_ms) {
        Ago::Now => text("island.staff_ago_now").to_string(),
        Ago::Minutes(n) => format("island.staff_ago_minutes", &[("n", n.to_string())]),
        Ago::Hours(n) => format("island.staff_ago_hours", &[("n", n.to_string())]),
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

impl IslandView {
    pub(super) fn staff_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if let Err(reason) = staff::availability(cx) {
            // Without any workspace the brain's state is unknown: the generic enable guide fits.
            let reason = (reason != "island.staff_no_workspace").then_some(reason);
            return self.staff_guide(reason, cx).into_any_element();
        }
        let input = self.staff_input(window, cx);
        staff::refresh_activity(cx);
        let workspace_count = self.controller.read(cx).visible_workspaces(cx).len();
        let state = staff::state(cx);
        let (lines, caption, notice, pending, activity, last_report) = {
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
                staff.report_pending(),
                staff.activity.clone(),
                staff.last_report.map(|at| at.elapsed().as_millis() as u64),
            )
        };
        if caption.is_some() {
            self.schedule_caption_tick(cx);
        }
        let mut subtitle = format(
            "island.staff_watching",
            &[("count", workspace_count.to_string())],
        );
        if let Some(elapsed) = last_report {
            subtitle += &format("island.staff_last_report", &[("ago", ago_label(elapsed))]);
        }
        let header = div()
            .flex()
            .items_center()
            .gap(px(12.))
            .child(
                div()
                    .size(px(44.))
                    .flex_none()
                    .rounded_full()
                    .bg(linear_gradient(
                        135.,
                        linear_color_stop(rgb(0xe3dcff), 0.),
                        linear_color_stop(rgb(0x8f7cff), 1.),
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(text("island.staff")),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(style::MUTED))
                            .truncate()
                            .child(subtitle),
                    ),
            )
            .child(
                style::action(
                    "island-staff-report",
                    format!("▶ {}", text("island.staff_report")),
                    Tone::Accent,
                    cx,
                )
                .disabled(pending)
                .on_click(cx.listener(|view, _, _, cx| view.request_staff_report(cx))),
            );
        let mut card = div()
            .id("island-staff-card")
            .flex()
            .flex_col()
            .gap(px(8.))
            .max_h(px(168.))
            .overflow_y_scroll()
            .p(px(14.))
            .rounded(px(16.))
            .bg(rgb(CARD_BG))
            .text_size(px(13.))
            .line_height(px(21.));
        if lines.is_empty() {
            card = card.child(
                div()
                    .text_color(rgb(style::MUTED))
                    .child(text("island.staff_no_summary")),
            );
        }
        for (id, role, body) in lines {
            let mine = role == StaffRole::User;
            card = card.child(
                div()
                    .id(SharedString::from(format!("island-staff-line-{id}")))
                    .text_color(rgb(if mine { style::MUTED } else { style::FG }))
                    .child(if mine {
                        format!("{}：{body}", text("island.staff_you"))
                    } else {
                        body
                    }),
            );
        }
        if let Some(caption) = caption {
            card = card.child(
                div()
                    .id("island-staff-caption")
                    .text_color(rgb(style::STAFF))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(caption),
            );
        }
        let mut panel = div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(header)
            .child(card);
        if let Some(notice) = notice {
            panel = panel.child(
                div()
                    .text_size(px(11.5))
                    .text_color(rgb(style::ATTENTION))
                    .child(notice.to_string()),
            );
        }
        if !activity.is_empty() {
            // Fixed height: rows arriving while the user reads must not resize the window.
            let mut log = div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .h(px(86.))
                .overflow_hidden()
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(rgb(style::FAINT))
                        .child(text("island.staff_activity")),
                );
            let now = now_ms();
            for line in activity {
                let when = ago_label(now.saturating_sub(line.at_ms.max(0) as u64));
                log = log.child(
                    div()
                        .flex()
                        .gap(px(12.))
                        .text_size(px(12.))
                        .child(div().flex_none().text_color(rgb(style::FAINT)).child(when))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .truncate()
                                .text_color(rgb(style::MUTED))
                                .child(line.label),
                        ),
                );
            }
            panel = panel.child(log);
        }
        let mut chips = div().flex().flex_wrap().gap(px(8.));
        for (id, key) in CHIPS {
            chips = chips.child(
                div()
                    .id(id)
                    .cursor_pointer()
                    .px(px(12.))
                    .py(px(6.))
                    .rounded_full()
                    .border_1()
                    .border_color(rgb(0x2a2a2a))
                    .text_size(px(12.))
                    .text_color(rgb(style::MUTED))
                    .hover(|chip| chip.bg(rgb(0x1d1d1d)).text_color(rgb(style::FG)))
                    .active(|chip| chip.bg(rgb(0x303030)))
                    .child(text(key))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        staff::send_message(text(key), cx);
                        cx.notify();
                    })),
            );
        }
        let send = {
            let input = input.clone();
            cx.listener(move |view, _, window, cx| view.submit_staff(&input, window, cx))
        };
        panel
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(style::FAINT))
                            .child(text("island.staff_chat_title")),
                    )
                    .child(chips),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|_, _, window, _| {
                                    super::focus::focus_panel_input(window);
                                }),
                            )
                            .child(
                                Input::new(&input)
                                    .w_full()
                                    .h(px(44.))
                                    .rounded(px(22.))
                                    .text_size(px(14.))
                                    .bg(rgb(CARD_BG)),
                            ),
                    )
                    .child(
                        Button::new("island-staff-send")
                            .w(px(44.))
                            .h(px(44.))
                            .p(px(0.))
                            .rounded(px(22.))
                            .child(
                                Icon::new(IconName::ArrowUp)
                                    .size(px(18.))
                                    .text_color(rgb(style::FG)),
                            )
                            .custom(
                                ButtonCustomVariant::new(cx)
                                    .color(rgb(0x1d1d1d).into())
                                    .border(rgba(0).into())
                                    .hover(rgb(0x303030).into())
                                    .active(rgb(0x404040).into()),
                            )
                            .on_click(send),
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
        // Keep the text when the brain refused it, so the user does not have to retype.
        if staff::send_message(&body, cx) {
            input.update(cx, |input, cx| input.set_value(String::new(), window, cx));
        }
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
