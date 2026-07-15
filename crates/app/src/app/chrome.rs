//! Workspace overlay chrome for the GPUI app shell.

mod dock;
mod top_bar;

use super::{
    MultisoulApp, WALLPAPER_IMAGE_OPACITY, WALLPAPER_SHADE_BOTTOM_ALPHA, WALLPAPER_SHADE_TOP_ALPHA,
};
use crate::app::wallpaper_resolvers::WallpaperKind;
use crate::canvas::minimap::render_canvas_minimap;
use gpui::{
    div, img, linear_color_stop, linear_gradient, point, prelude::*, px, rgba, BoxShadow, Context,
    Div, ObjectFit, Window,
};
use ui::icons::with_alpha;
use ui::Theme;

const WORKSPACE_LEFT_RAIL_VISIBLE: bool = false;
const BOTTOM_VOICE_BAR_VISIBLE: bool = true;
const WALLPAPER_FROST_SAMPLE_ALPHA: u8 = 0x24;
const FROST_SAMPLE_OFFSETS: [(f32, f32); 5] = [
    (0.0, 0.0),
    (-5.0, -3.0),
    (5.0, 3.0),
    (-3.0, 5.0),
    (3.0, -5.0),
];

impl MultisoulApp {
    pub(super) fn render_workspace_backdrop(&self) -> Div {
        let theme = self.theme;
        let mut backdrop = div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .overflow_hidden()
            .bg(linear_gradient(
                150.0,
                linear_color_stop(rgba(theme.canvas_from), 0.0),
                linear_color_stop(rgba(theme.graphite), 1.0),
            ));

        backdrop = backdrop.child(self.render_wallpaper_layer());

        backdrop
            .child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .right(px(0.0))
                    .bottom(px(0.0))
                    .bg(linear_gradient(
                        180.0,
                        linear_color_stop(
                            rgba(with_alpha(theme.graphite, WALLPAPER_SHADE_TOP_ALPHA)),
                            0.0,
                        ),
                        linear_color_stop(
                            rgba(with_alpha(theme.graphite, WALLPAPER_SHADE_BOTTOM_ALPHA)),
                            1.0,
                        ),
                    )),
            )
            .child(
                div()
                    .absolute()
                    .left(px(-160.0))
                    .top(px(54.0))
                    .w(px(520.0))
                    .h(px(520.0))
                    .rounded(px(260.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.accent_cyan, 0x24))),
            )
            .child(
                div()
                    .absolute()
                    .right(px(-180.0))
                    .bottom(px(-120.0))
                    .w(px(620.0))
                    .h(px(620.0))
                    .rounded(px(310.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.accent_orange, 0x2e))),
            )
            .child(
                div()
                    .absolute()
                    .left(px(90.0))
                    .right(px(90.0))
                    .bottom(px(84.0))
                    .h(px(1.0))
                    .bg(rgba(with_alpha(theme.accent_cyan, 0x18))),
            )
    }

    /// Render the wallpaper layer, branching on the current [`WallpaperKind`].
    ///
    /// - `Video` (mac): render the live decoded frame via `surface()`; until
    ///   the first frame is decoded (or on non-mac) fall back to the default
    ///   image so the canvas never shows an empty backdrop.
    /// - `Static` / `Gif`: render the image via `img()` with a source-derived
    ///   id (required both for GPUI GIF multi-frame animation and to avoid the
    ///   cross-source `frame_index` out-of-bounds panic; see `wallpaper_img`).
    pub(crate) fn render_wallpaper_layer(&self) -> Div {
        let host = div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .size_full()
            .overflow_hidden();

        match self.wallpaper_settings.current_kind() {
            WallpaperKind::Video => {
                #[cfg(target_os = "macos")]
                if let Some(frame) = self.render_video_wallpaper_surface() {
                    return host.child(frame);
                }
                host.child(self.wallpaper_img())
            }
            WallpaperKind::Static | WallpaperKind::Gif => host.child(self.wallpaper_img()),
        }
    }

    /// The default image wallpaper element (Static/Gif + Video fallback).
    /// Its id is derived from the rendered source (`render_id_key`) — NOT a
    /// fixed string — so GPUI discards the stale `ImgState` (and its
    /// `frame_index`) when the wallpaper source changes. See the inline note
    /// below for the panic this prevents.
    fn wallpaper_img(&self) -> gpui::Stateful<gpui::Img> {
        // The id MUST be derived from the rendered source, not a fixed string.
        // GPUI 0.2.2 keeps `ImgState.frame_index` per element-id across renders;
        // a multi-frame GIF can push frame_index past 1, and reusing the same
        // id when switching to a single-frame image lets the stale index access
        // the new 1-frame RenderImage.data → index-out-of-bounds panic
        // (assets.rs:79). A source-derived id makes GPUI treat it as a fresh
        // element and reset frame_index to 0. The id is still stable per source
        // so the GIF multi-frame animation path stays active across repaints.
        img(self.wallpaper_settings.image_source_for_render())
            .id(gpui::SharedString::from(
                self.wallpaper_settings.render_id_key(),
            ))
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .size_full()
            .object_fit(ObjectFit::Cover)
            .opacity(WALLPAPER_IMAGE_OPACITY)
    }

    /// Crop the active wallpaper to a card's screen-space bounds and blend
    /// five nearby samples. This mirrors a CSS backdrop blur's spatial mixing
    /// without reading pixels back to the CPU. Static/GIF samples reuse GPUI's
    /// image cache; video samples reuse the current 420f CVPixelBuffer surface.
    pub(crate) fn render_card_frosted_backdrop(
        &self,
        node_id: canvas::NodeId,
        card_left: f32,
        card_top: f32,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Div {
        let kind = self.wallpaper_settings.current_kind();
        let source = self.wallpaper_settings.image_source_for_render();
        let render_key = self.wallpaper_settings.render_id_key();
        let sample_opacity = f32::from(WALLPAPER_FROST_SAMPLE_ALPHA) / 255.0;
        let mut backdrop = div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .size_full()
            .overflow_hidden();

        for (sample_index, (dx, dy)) in FROST_SAMPLE_OFFSETS.into_iter().enumerate() {
            let sample_left = -card_left + dx;
            let sample_top = -card_top + dy;

            #[cfg(target_os = "macos")]
            if kind == WallpaperKind::Video {
                if let Some(sample) = self.render_video_wallpaper_frost_sample(
                    sample_left,
                    sample_top,
                    viewport_width,
                    viewport_height,
                    sample_opacity,
                ) {
                    backdrop = backdrop.child(sample);
                    continue;
                }
            }

            backdrop = backdrop.child(
                img(source.clone())
                    .id(gpui::SharedString::from(format!(
                        "card-frost:{render_key}:{node_id}:{sample_index}"
                    )))
                    .absolute()
                    .left(px(sample_left))
                    .top(px(sample_top))
                    .w(px(viewport_width))
                    .h(px(viewport_height))
                    .object_fit(ObjectFit::Cover)
                    .opacity(sample_opacity),
            );
        }

        backdrop.child(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .bg(linear_gradient(
                    155.0,
                    linear_color_stop(rgba(0xf8fafd24), 0.0),
                    linear_color_stop(rgba(0xe8eef82e), 1.0),
                )),
        )
    }

    pub(super) fn render_workspace_chrome(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.theme;

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .child(render_canvas_minimap(&self.runtime, theme, window))
            .child(self.render_command_dock(window, cx))
            .child(self.render_workspace_top_bar(cx))
            .when(workspace_left_rail_visible(), |chrome| {
                chrome.child(
                    div()
                        .absolute()
                        .left(px(24.0))
                        .top(px(178.0))
                        .w(px(46.0))
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_2()
                        .p(px(8.0))
                        .rounded(px(23.0))
                        .border_1()
                        .border_color(rgba(with_alpha(theme.edge_highlight, 0x2f)))
                        .shadow(chrome_shadow(theme))
                        .bg(rgba(with_alpha(theme.panel, 0xd0)))
                        .child(chrome_button(theme, ">", true))
                        .child(chrome_button(theme, "H", false))
                        .child(chrome_button(theme, "[]", false))
                        .child(chrome_button(theme, "<>", false))
                        .child(chrome_button(theme, "O", false))
                        .child(chrome_button(theme, "T", false))
                        .child(chrome_button(theme, "AI", false))
                        .child(chrome_button(theme, "{}", false)),
                )
            })
            .when(self.interaction.new_menu_open, |chrome| {
                chrome.child(self.render_new_menu(cx))
            })
    }
}

pub(super) fn chrome_shadow(theme: Theme) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: rgba(theme.shadow_color).into(),
        offset: point(px(0.0), px(18.0)),
        blur_radius: px(36.0),
        spread_radius: px(-18.0),
    }]
}

fn chrome_button(theme: Theme, label: &'static str, active: bool) -> Div {
    let fill = if active {
        with_alpha(theme.accent_blue, 0x72)
    } else {
        0xffffff0a
    };
    let stroke = if active {
        with_alpha(theme.accent_cyan, 0x8c)
    } else {
        0xffffff18
    };
    let text = if active { 0xffffffff } else { 0xffffffc8 };

    div()
        .w(px(chrome_button_width(label)))
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(stroke))
        .bg(rgba(fill))
        .text_color(rgba(text))
        .text_size(px(11.0))
        .child(label)
}

fn chrome_button_width(label: &str) -> f32 {
    let text_width = label.chars().count() as f32 * 6.4;
    (text_width + 18.0).max(28.0).ceil()
}

fn workspace_left_rail_visible() -> bool {
    WORKSPACE_LEFT_RAIL_VISIBLE
}

fn bottom_voice_bar_visible() -> bool {
    BOTTOM_VOICE_BAR_VISIBLE
}

#[cfg(test)]
#[path = "../../tests/unit/app_chrome_tests.rs"]
mod app_chrome_tests;

#[cfg(test)]
#[path = "../../tests/unit/app_chrome_cursor_tests.rs"]
mod app_chrome_cursor_tests;

#[cfg(test)]
#[path = "../../tests/unit/product_ui_chrome_tests.rs"]
mod product_ui_chrome_tests;
