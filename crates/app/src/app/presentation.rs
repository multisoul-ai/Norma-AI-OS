use super::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, Stateful};
use ui::icons::with_alpha;

const PRESENTATION_NOTIFICATION_DURATION: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PresentationNotification {
    pub(crate) source_name: String,
    pub(crate) count: usize,
}

impl MultisoulApp {
    pub(crate) fn show_presentation_notification(
        &mut self,
        source_name: String,
        cx: &mut Context<Self>,
    ) {
        self.presentation_notification_generation =
            self.presentation_notification_generation.saturating_add(1);
        let generation = self.presentation_notification_generation;
        match self.presentation_notification.as_mut() {
            Some(notification) if notification.source_name == source_name => {
                notification.count = notification.count.saturating_add(1);
            }
            _ => {
                self.presentation_notification = Some(PresentationNotification {
                    source_name,
                    count: 1,
                });
            }
        }

        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(PRESENTATION_NOTIFICATION_DURATION)
                .await;
            let _ = this.update(cx, |app, cx| {
                if app.presentation_notification_generation == generation {
                    app.presentation_notification = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn render_presentation_notification(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.theme;
        let notification = self
            .presentation_notification
            .as_ref()
            .expect("render is gated by presentation_notification presence");
        let item_label = if notification.count == 1 {
            "item"
        } else {
            "items"
        };
        div()
            .id("presentation-notification")
            .debug_selector(|| "presentation-notification".to_string())
            .absolute()
            .top(px(22.0))
            .right(px(22.0))
            .max_w(px(360.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .p(px(13.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(rgba(with_alpha(theme.accent_blue, 0x70)))
            .bg(rgba(with_alpha(theme.panel, 0xf2)))
            .text_color(rgba(theme.text_primary))
            .child(
                div()
                    .w(px(28.0))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.0))
                    .bg(rgba(with_alpha(theme.accent_blue, 0x24)))
                    .text_color(rgba(theme.accent_blue))
                    .child("↗"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(ui::i18n::format(
                        "chrome.presented_item_label",
                        &[
                            ("arg0", (notification.source_name).to_string()),
                            ("arg1", (notification.count).to_string()),
                            ("item_label", (item_label).to_string()),
                        ],
                    ))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgba(theme.text_secondary))
                            .child(ui::i18n::text("chrome.shown_in_master_layout")),
                    ),
            )
            .child(
                div()
                    .id("presentation-notification-close")
                    .debug_selector(|| "presentation-notification-close".to_string())
                    .cursor_pointer()
                    .text_color(rgba(theme.text_secondary))
                    .child("×")
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.presentation_notification = None;
                        cx.notify();
                    })),
            )
    }
}
