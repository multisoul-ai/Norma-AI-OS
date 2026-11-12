//! 语音光晕的迷你预览：默认壁纸静帧 + 示意 Dock + 与真实窗口同一套图元。

use crate::app::voice_window_inset::paint_prims;
use crate::assets::DEFAULT_WALLPAPER_STILL_ASSET;
use crate::effects::voice_glow::preview::{self, PreviewPin, VIRTUAL_H, VIRTUAL_W};
use crate::effects::voice_glow::VoiceGlowStyle;
use gpui::{canvas, div, img, prelude::*, px, rgba, ContentMask, Div, ImageSource, ObjectFit};
use ui::Theme;

pub(super) struct GlowScene {
    pub theme: Theme,
    pub style: VoiceGlowStyle,
    pub pin: Option<PreviewPin>,
    pub t_ms: f32,
    pub gain: f32,
    pub width: f32,
    pub playing: bool,
    pub reduce_motion: bool,
}

pub(super) fn glow_scene(scene: GlowScene) -> Div {
    let GlowScene {
        theme,
        style,
        pin,
        t_ms,
        gain,
        width,
        playing,
        reduce_motion,
    } = scene;
    let scale = width / VIRTUAL_W;
    let height = VIRTUAL_H * scale;
    let prims = preview::prims(style, pin, t_ms, reduce_motion, gain);
    let dock = preview::dock();
    div()
        .relative()
        .flex_none()
        .w(px(width))
        .h(px(height))
        .overflow_hidden()
        .rounded(px(10.0))
        .child(
            img(ImageSource::from(DEFAULT_WALLPAPER_STILL_ASSET))
                .w(px(width))
                .h(px(height))
                .rounded(px(10.0))
                .object_fit(ObjectFit::Cover),
        )
        .child(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .w(px(width))
                .h(px(height))
                .bg(rgba(0x05050722)),
        )
        .child(
            div()
                .absolute()
                .left(px(dock.x * scale))
                .top(px(dock.y * scale))
                .w(px(dock.w * scale))
                .h(px(dock.h * scale))
                .rounded_full()
                .border_1()
                .border_color(rgba((theme.node_stroke & 0xffffff00) | 0x60))
                .bg(rgba((theme.panel_high & 0xffffff00) | 0xb0)),
        )
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    // 柔光的模糊会越出缩略图，必须裁在预览范围内。
                    window.with_content_mask(Some(ContentMask { bounds }), |window| {
                        paint_prims(window, bounds.origin, scale, &prims);
                    });
                    if playing {
                        window.request_animation_frame();
                    }
                },
            )
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .w(px(width))
            .h(px(height)),
        )
}
