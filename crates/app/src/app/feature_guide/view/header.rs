use super::*;

impl MultisoulApp {
    pub(super) fn render_feature_guide_header(
        &self,
        theme: Theme,
        palette: FeatureGuidePalette,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .h(px(72.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .px(px(26.0))
            .border_b_1()
            .border_color(rgba(guide_stroke(theme, 0x80)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .w(px(34.0))
                            .h(px(34.0))
                            .rounded(px(11.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(rgba(with_alpha(theme.accent_blue, 0x22)))
                            .text_color(rgba(palette.accent_text))
                            .font_weight(FontWeight::BOLD)
                            .child("N"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgba(palette.primary_text))
                                    .child(ui::i18n::text("guide.feature_guide")),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "feature-guide-progress".to_string())
                                    .text_size(px(11.0))
                                    .text_color(rgba(palette.metadata_text))
                                    .child(format!(
                                        "{} / {}",
                                        self.feature_guide.topic_position(),
                                        FeatureGuideTopic::ALL.len()
                                    )),
                            ),
                    ),
            )
            .child(guide_button(
                GuideButtonKind::Close,
                ui::i18n::text("guide.close_esc"),
                theme,
                palette,
                cx,
                |app, cx| app.close_feature_guide_for_session(cx),
            ))
    }
}
