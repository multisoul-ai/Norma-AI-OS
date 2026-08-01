use super::style::FeatureGuidePalette;
use super::FeatureGuideTopic;
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, AnyElement, BoxShadow, Context, Div, FontWeight};
use ui::Theme;

pub(super) fn render_demo(
    topic: FeatureGuideTopic,
    frame: usize,
    reduce_motion: bool,
    theme: Theme,
    palette: FeatureGuidePalette,
    _cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    let effective_frame = if reduce_motion { 2 } else { frame };
    let stage = div()
        .id("feature-guide-demo")
        .debug_selector(|| "feature-guide-demo".to_string())
        .w_full()
        .h(px(190.0))
        .flex_none()
        .relative()
        .overflow_hidden()
        .rounded(px(18.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.accent_blue, 0x38)))
        .bg(rgba(with_alpha(theme.graphite, 0x0d)));
    match topic {
        FeatureGuideTopic::VoiceDispatch => stage
            .child(demo_card(58.0, 34.0, "小张 · Ready", theme, palette))
            .child(demo_voice_bar(effective_frame, theme))
            .child(demo_meteor(effective_frame, theme)),
        FeatureGuideTopic::ContinueConversation => stage
            .child(demo_card(44.0, 30.0, "小张 · Working", theme, palette))
            .child(demo_bubble(
                270.0,
                52.0,
                "再补充一个验收条件",
                theme.accent_blue,
                theme,
                palette,
            ))
            .child(demo_bubble(
                330.0,
                108.0,
                if effective_frame == 0 {
                    "正在接收…"
                } else {
                    "已加入当前会话"
                },
                theme.accent_green,
                theme,
                palette,
            )),
        FeatureGuideTopic::CanvasManagement => stage
            .child(demo_card(
                34.0 + effective_frame as f32 * 18.0,
                28.0,
                "Terminal",
                theme,
                palette,
            ))
            .child(demo_card(
                250.0,
                92.0 - effective_frame as f32 * 18.0,
                "Agent · 小张",
                theme,
                palette,
            ))
            .child(demo_card(
                472.0 - effective_frame as f32 * 22.0,
                38.0,
                "Preview",
                theme,
                palette,
            )),
        FeatureGuideTopic::TaskStatus => stage
            .child(demo_card(48.0, 35.0, "小张 · Working", theme, palette))
            .child(demo_bubble(
                280.0,
                42.0,
                "● Working",
                theme.accent_green,
                theme,
                palette,
            ))
            .child(demo_bubble(
                280.0,
                90.0,
                "2 Queued",
                theme.accent_violet,
                theme,
                palette,
            ))
            .child(demo_bubble(
                410.0,
                132.0,
                if effective_frame < 2 {
                    "派发中…"
                } else {
                    "✓ 已送达"
                },
                theme.accent_blue,
                theme,
                palette,
            )),
    }
    .into_any_element()
}

fn demo_card(
    left: f32,
    top: f32,
    label: &'static str,
    theme: Theme,
    palette: FeatureGuidePalette,
) -> Div {
    div()
        .absolute()
        .left(px(left))
        .top(px(top))
        .w(px(190.0))
        .h(px(104.0))
        .rounded(px(14.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.node_stroke, 0xc0)))
        .bg(rgba(with_alpha(theme.panel_high, 0xe8)))
        .shadow(vec![BoxShadow {
            color: rgba(with_alpha(theme.shadow_color, 0x40)).into(),
            offset: gpui::point(px(0.0), px(8.0)),
            blur_radius: px(20.0),
            spread_radius: px(-8.0),
        }])
        .child(
            div()
                .h(px(34.0))
                .px(px(12.0))
                .flex()
                .items_center()
                .border_b_1()
                .border_color(rgba(with_alpha(theme.node_stroke, 0x70)))
                .text_size(px(11.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(palette.primary_text))
                .child(label),
        )
        .child(
            div()
                .m(px(12.0))
                .h(px(8.0))
                .w(px(120.0))
                .rounded(px(4.0))
                .bg(rgba(with_alpha(theme.text_tertiary, 0x22))),
        )
}

fn demo_voice_bar(frame: usize, theme: Theme) -> Div {
    div()
        .absolute()
        .left(px(52.0))
        .bottom(px(18.0))
        .w(px(258.0))
        .h(px(38.0))
        .px(px(12.0))
        .flex()
        .items_center()
        .gap(px(10.0))
        .rounded(px(19.0))
        .bg(rgba(with_alpha(theme.graphite, 0xd8)))
        .text_color(rgba(0xffffffff))
        .text_size(px(10.0))
        .child(if frame == 0 {
            "● 正在聆听"
        } else {
            "◉ 给小张：整理帮助页面"
        })
}

fn demo_meteor(frame: usize, theme: Theme) -> Div {
    div()
        .absolute()
        .left(px(330.0 + frame as f32 * 92.0))
        .top(px(118.0 - frame as f32 * 38.0))
        .w(px(96.0))
        .h(px(3.0))
        .rounded(px(2.0))
        .bg(rgba(with_alpha(theme.accent_cyan, 0xe8)))
        .child(
            div()
                .absolute()
                .right(px(-4.0))
                .top(px(-4.0))
                .w(px(11.0))
                .h(px(11.0))
                .rounded(px(6.0))
                .bg(rgba(theme.accent_cyan)),
        )
}

fn demo_bubble(
    left: f32,
    top: f32,
    label: &'static str,
    accent: u32,
    theme: Theme,
    palette: FeatureGuidePalette,
) -> Div {
    div()
        .absolute()
        .left(px(left))
        .top(px(top))
        .px(px(13.0))
        .h(px(36.0))
        .flex()
        .items_center()
        .rounded(px(18.0))
        .border_1()
        .border_color(rgba(with_alpha(accent, 0x70)))
        .bg(rgba(with_alpha(theme.panel_high, 0xf0)))
        .text_size(px(10.0))
        .text_color(rgba(palette.status_text))
        .child(label)
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
