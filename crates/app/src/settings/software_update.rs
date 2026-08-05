//! Settings → Software Update, backed by Sparkle's standard update user flow.

use gpui::{div, prelude::*, px, rgba, Div, FontWeight};
use ui::component::button::{Button, ButtonVariants};
use ui::component::{Sizable as _, Size};
use ui::Theme;

pub(super) fn software_update_settings(theme: Theme) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .text_size(px(15.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgba(theme.text_primary))
                        .child("Update Norma OS"),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgba(theme.text_tertiary))
                        .child("Check GitHub Releases for a newer signed version."),
                ),
        )
        .child(
            div()
                .w_full()
                .min_h(px(118.0))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(20.0))
                .px(px(18.0))
                .py(px(16.0))
                .rounded(px(16.0))
                .border_1()
                .border_color(rgba(theme.node_stroke))
                .bg(rgba(theme.panel_high))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap(px(7.0))
                        .child(
                            div()
                                .id("settings-software-update-current-version")
                                .debug_selector(|| {
                                    "settings-software-update-current-version".to_string()
                                })
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(9.0))
                                .child(
                                    div()
                                        .text_size(px(14.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(rgba(theme.text_primary))
                                        .child("Current Version"),
                                )
                                .child(
                                    div()
                                        .px(px(8.0))
                                        .py(px(3.0))
                                        .rounded(px(9.0))
                                        .bg(rgba(theme.panel))
                                        .text_size(px(10.0))
                                        .font_family("Menlo")
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(rgba(theme.text_secondary))
                                        .child(env!("CARGO_PKG_VERSION")),
                                ),
                        )
                        .child(
                            div()
                                .max_w(px(590.0))
                                .text_size(px(12.0))
                                .line_height(px(18.0))
                                .text_color(rgba(theme.text_tertiary))
                                .child(
                                    "If an update is available, Sparkle verifies and downloads it. When it is ready to install, Norma OS asks you to restart the app.",
                                ),
                        ),
                )
                .child(
                    Button::new("settings-check-for-updates")
                        .debug_selector(|| "settings-check-for-updates".to_string())
                        .label("Check for Updates…")
                        .primary()
                        .with_size(Size::Medium)
                        .tooltip("Check for a newer signed Norma OS release")
                        .on_click(|_, _window, cx| {
                            crate::updater::check_for_updates(cx);
                        }),
                ),
        )
}
