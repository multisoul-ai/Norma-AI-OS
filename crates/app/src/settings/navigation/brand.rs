use super::*;
use crate::settings::typography::{SettingsText, TextRole};

pub(super) fn settings_brand_footer(theme: Theme, compact: bool) -> Stateful<Div> {
    let brand = settings_brand_metadata();
    let icon_size = if compact { 36.0 } else { 40.0 };
    let icon_radius = if compact { 10.0 } else { 12.0 };

    div()
        .id("settings-brand-footer")
        .debug_selector(|| "settings-brand-footer".to_string())
        .w_full()
        .flex_none()
        .mt(px(if compact { 10.0 } else { 16.0 }))
        .pt(px(if compact { 10.0 } else { 14.0 }))
        .border_t_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x2e)))
        .child(
            div()
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(if compact { 8.0 } else { 11.0 }))
                .p(px(if compact { 8.0 } else { 10.0 }))
                .rounded(px(14.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.node_stroke, 0x88)))
                .bg(rgba(with_alpha(theme.panel_high, 0x78)))
                .child(
                    div()
                        .id("settings-brand-icon")
                        .debug_selector(|| "settings-brand-icon".to_string())
                        .w(px(icon_size))
                        .h(px(icon_size))
                        .flex_none()
                        .rounded(px(icon_radius))
                        .border_1()
                        .border_color(rgba(with_alpha(theme.edge_highlight, 0xa8)))
                        .child(
                            img(brand.icon_asset)
                                .size_full()
                                .object_fit(ObjectFit::Cover)
                                .rounded(px(icon_radius)),
                        ),
                )
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .whitespace_normal()
                                .settings_text(TextRole::RowTitle, theme)
                                .child(brand.product_name),
                        )
                        .child(
                            div()
                                .id("settings-brand-version")
                                .debug_selector(|| "settings-brand-version".to_string())
                                .whitespace_normal()
                                .settings_font(TextRole::Caption)
                                .text_color(rgba(theme.text_tertiary))
                                .child(ui::i18n::format(
                                    "settings.version",
                                    &[("arg0", (brand.version).to_string())],
                                )),
                        ),
                )
                .child(
                    Button::new("settings-check-for-updates")
                        .debug_selector(|| "settings-check-for-updates".to_string())
                        .icon(Icon::default().path(SETTINGS_CHECK_UPDATES_ASSET))
                        .ghost()
                        .with_size(Size::Medium)
                        .tooltip(ui::i18n::text("settings.check_for_updates"))
                        .on_click(|_, _window, cx| {
                            crate::updater::check_for_updates(cx);
                        }),
                ),
        )
}
