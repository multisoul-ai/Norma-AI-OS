use super::demo::render_demo;
use super::style::{guide_shadow, with_alpha, FeatureGuidePalette};
use super::FeatureGuideTopic;
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, AnyElement, Context, Div, FontWeight, MouseButton, Window};
use ui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use ui::Theme;

impl MultisoulApp {
    pub(crate) fn render_feature_guide(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let palette = FeatureGuidePalette::from_theme(theme);
        let viewport = window.viewport_size();
        let available_width = (f32::from(viewport.width) - 24.0).max(1.0);
        let available_height = (f32::from(viewport.height) - 24.0).max(1.0);
        let width = available_width
            .min(1_080.0)
            .max(680.0_f32.min(available_width));
        let height = available_height
            .min(760.0)
            .max(560.0_f32.min(available_height));
        let panel_left = ((f32::from(viewport.width) - width) / 2.0).max(0.0);
        let panel_top = ((f32::from(viewport.height) - height) / 2.0).max(0.0);
        let compact = width < 820.0;
        let topic = self.feature_guide.topic;
        #[cfg(feature = "e2e-test-support")]
        {
            self.feature_guide_primary_focused_for_e2e =
                self.feature_guide_primary_focus.is_focused(window);
        }

        div()
            .id("feature-guide-overlay")
            .debug_selector(|| "feature-guide-overlay".to_string())
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .size_full()
            .occlude()
            .bg(rgba(with_alpha(theme.graphite, 0x72)))
            .on_mouse_down(MouseButton::Left, {
                let focus = self.focus_handle.clone();
                move |_, window, cx| {
                    focus.focus(window);
                    cx.stop_propagation();
                }
            })
            .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("feature-guide-panel")
                    .debug_selector(|| "feature-guide-panel".to_string())
                    .track_focus(&self.focus_handle)
                    .absolute()
                    .left(px(panel_left))
                    .top(px(panel_top))
                    .w(px(width))
                    .h(px(height))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .rounded(px(24.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.node_stroke, 0xd8)))
                    .bg(rgba(with_alpha(theme.panel_high, 0xfc)))
                    .shadow(guide_shadow(theme))
                    .child(self.render_feature_guide_header(topic, theme, palette, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_h(px(0.0))
                            .flex()
                            .when(compact, |body| body.flex_col())
                            .when(!compact, |body| body.flex_row())
                            .child(self.render_feature_guide_navigation(
                                topic, compact, theme, palette, cx,
                            ))
                            .child(
                                self.render_feature_guide_content(
                                    topic, compact, theme, palette, cx,
                                ),
                            ),
                    )
                    .child(self.render_feature_guide_footer(topic, theme, palette, window, cx)),
            )
            .into_any_element()
    }

    fn render_feature_guide_header(
        &self,
        topic: FeatureGuideTopic,
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
            .border_color(rgba(with_alpha(theme.node_stroke, 0x80)))
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
                                    .child("Feature Guide"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(rgba(palette.metadata_text))
                                    .child(format!(
                                        "{} / {}",
                                        topic_index(topic) + 1,
                                        FeatureGuideTopic::ALL.len()
                                    )),
                            ),
                    ),
            )
            .child(guide_button(
                "feature-guide-close",
                "Close  Esc",
                theme,
                palette,
                cx,
                |app, cx| app.close_feature_guide_for_session(cx),
            ))
    }

    fn render_feature_guide_navigation(
        &self,
        active: FeatureGuideTopic,
        compact: bool,
        theme: Theme,
        palette: FeatureGuidePalette,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let mut navigation = div()
            .id("feature-guide-navigation")
            .debug_selector(|| "feature-guide-navigation".to_string())
            .flex_none()
            .flex()
            .gap(px(if compact { 6.0 } else { 4.0 }))
            .bg(rgba(with_alpha(theme.graphite, 0x0a)));
        navigation = if compact {
            navigation
                .h(px(72.0))
                .w_full()
                .flex_row()
                .items_center()
                .px(px(18.0))
                .border_b_1()
        } else {
            navigation
                .w(px(220.0))
                .h_full()
                .flex_col()
                .p(px(14.0))
                .border_r_1()
        }
        .border_color(rgba(with_alpha(theme.node_stroke, 0x68)));

        for (index, topic) in FeatureGuideTopic::ALL.into_iter().enumerate() {
            let selected = active == topic;
            let selector = format!("feature-guide-topic-{index}");
            let tone = if selected {
                palette.primary_text
            } else {
                palette.supporting_text
            };
            let variant = ButtonCustomVariant::new(cx)
                .color(
                    rgba(if selected {
                        with_alpha(theme.accent_blue, 0x14)
                    } else {
                        0x00000000
                    })
                    .into(),
                )
                .foreground(rgba(tone).into())
                .border(rgba(0x00000000).into())
                .hover(rgba(with_alpha(theme.accent_blue, 0x0e)).into())
                .active(rgba(with_alpha(theme.accent_blue, 0x20)).into());
            let label = if compact {
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.0))
                    .font_weight(if selected {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(rgba(tone))
                    .child((index + 1).to_string())
            } else {
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .text_color(rgba(tone))
                    .child(
                        div()
                            .w(px(20.0))
                            .h(px(20.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.0))
                            .bg(rgba(if selected {
                                with_alpha(theme.accent_blue, 0x18)
                            } else {
                                0x00000000
                            }))
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child((index + 1).to_string()),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .truncate()
                            .text_size(px(13.0))
                            .font_weight(if selected {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .child(topic.navigation_label()),
                    )
            };
            navigation = navigation.child(
                Button::new(("feature-guide-topic", index))
                    .debug_selector(move || selector.clone())
                    .child(label)
                    .tooltip(topic.title())
                    .tab_index(index as isize + 2)
                    .custom(variant)
                    .when(compact, |item| item.flex_1().h(px(40.0)))
                    .when(!compact, |item| item.w_full().h(px(44.0)).px(px(10.0)))
                    .rounded(px(10.0))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        cx.stop_propagation();
                        app.select_feature_guide_topic(topic, cx);
                    })),
            );
        }
        navigation
    }

    fn render_feature_guide_content(
        &self,
        topic: FeatureGuideTopic,
        compact: bool,
        theme: Theme,
        palette: FeatureGuidePalette,
        _cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .gap(px(if compact { 12.0 } else { 16.0 }))
            .px(px(if compact { 22.0 } else { 30.0 }))
            .py(px(if compact { 16.0 } else { 22.0 }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgba(palette.accent_text))
                            .child(topic.eyebrow()),
                    )
                    .child(
                        div()
                            .id("feature-guide-title")
                            .debug_selector(|| "feature-guide-title".to_string())
                            .text_size(px(if compact { 21.0 } else { 25.0 }))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgba(palette.primary_text))
                            .child(topic.title()),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgba(palette.supporting_text))
                            .child(topic.summary()),
                    ),
            )
            .child(render_demo(
                topic,
                self.feature_guide_media.poster_path.clone(),
                self.feature_guide_media.video_surface(),
                compact,
                self.motion.reduce_motion,
                self.feature_guide_media.playing,
                theme,
                palette,
            ))
    }

    fn render_feature_guide_footer(
        &self,
        topic: FeatureGuideTopic,
        theme: Theme,
        palette: FeatureGuidePalette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let is_last = topic == FeatureGuideTopic::AgentNotifications;
        let action_label = if is_last { "Get Started" } else { "Next  →" };
        div()
            .h(px(68.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .px(px(26.0))
            .border_t_1()
            .border_color(rgba(with_alpha(theme.node_stroke, 0x80)))
            .child(guide_button(
                "feature-guide-skip",
                "Skip Guide",
                theme,
                palette,
                cx,
                |app, cx| app.suppress_feature_guide(cx),
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .when(!self.motion.reduce_motion, |actions| {
                        actions.child(guide_button(
                            "feature-guide-playback",
                            if self.feature_guide_media.playing {
                                "Pause Demo"
                            } else {
                                "Play Demo"
                            },
                            theme,
                            palette,
                            cx,
                            |app, cx| app.toggle_feature_guide_demo_playback(cx),
                        ))
                    })
                    .when(self.motion.reduce_motion, |actions| {
                        actions.child(
                            div()
                                .debug_selector(|| "feature-guide-reduced-motion".to_string())
                                .px(px(10.0))
                                .text_size(px(11.0))
                                .text_color(rgba(palette.metadata_text))
                                .child("Reduced Motion"),
                        )
                    })
                    .child(
                        div()
                            .track_focus(&self.feature_guide_primary_focus)
                            .rounded(px(11.0))
                            .when(
                                self.feature_guide_primary_focus.is_focused(window),
                                |wrapper| {
                                    wrapper
                                        .border_1()
                                        .border_color(rgba(with_alpha(theme.accent_blue, 0xc8)))
                                },
                            )
                            .child(guide_button(
                                "feature-guide-primary",
                                action_label,
                                theme,
                                palette,
                                cx,
                                move |app, cx| {
                                    if is_last {
                                        app.complete_feature_guide(cx);
                                    } else {
                                        app.next_feature_guide_topic(cx);
                                    }
                                },
                            )),
                    ),
            )
    }
}

fn guide_button(
    selector: &'static str,
    label: &'static str,
    theme: Theme,
    palette: FeatureGuidePalette,
    cx: &mut Context<MultisoulApp>,
    handler: impl Fn(&mut MultisoulApp, &mut Context<MultisoulApp>) + 'static,
) -> AnyElement {
    let is_primary = selector == "feature-guide-primary";
    let tab_index = match selector {
        "feature-guide-primary" => 1,
        "feature-guide-playback" => 7,
        "feature-guide-skip" => 8,
        "feature-guide-close" => 9,
        _ => 10,
    };
    let background = if is_primary {
        palette.primary_background
    } else {
        with_alpha(theme.text_primary, 0x0d)
    };
    let foreground = if is_primary {
        palette.primary_foreground
    } else {
        palette.supporting_text
    };
    let hover = if is_primary {
        palette.primary_hover
    } else {
        with_alpha(theme.text_primary, 0x18)
    };
    let active = if is_primary {
        palette.primary_active
    } else {
        with_alpha(theme.text_primary, 0x24)
    };
    let variant = ButtonCustomVariant::new(cx)
        .color(rgba(background).into())
        .foreground(rgba(foreground).into())
        .border(rgba(with_alpha(theme.accent_blue, 0x58)).into())
        .hover(rgba(hover).into())
        .active(rgba(active).into());
    Button::new(selector)
        .debug_selector(move || selector.to_string())
        .child(div().text_color(rgba(foreground)).child(label))
        .tab_index(tab_index)
        .tab_stop(!is_primary)
        .custom(variant)
        .h(px(36.0))
        .px(px(14.0))
        .rounded(px(10.0))
        .border_color(rgba(with_alpha(theme.node_stroke, 0x78)))
        .on_click(cx.listener(move |app, _, _, cx| {
            cx.stop_propagation();
            handler(app, cx);
        }))
        .into_any_element()
}

fn topic_index(topic: FeatureGuideTopic) -> usize {
    FeatureGuideTopic::ALL
        .iter()
        .position(|candidate| *candidate == topic)
        .unwrap_or(0)
}
