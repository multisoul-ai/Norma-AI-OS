//! Workspace overlay chrome for the GPUI app shell.

mod dock;
mod top_bar;

use super::{MultisoulApp, WALLPAPER_IMAGE_OPACITY};
use crate::app::wallpaper_resolvers::WallpaperKind;
use crate::canvas::minimap::render_canvas_minimap;
use gpui::{
    div, img, linear_color_stop, linear_gradient, point, prelude::*, px, rgba, BoxShadow, Context,
    Div, ObjectFit, Stateful, Window,
};
use ui::icons::with_alpha;
use ui::Theme;

const WORKSPACE_LEFT_RAIL_VISIBLE: bool = false;
const BOTTOM_VOICE_BAR_VISIBLE: bool = true;
const WORKSPACE_ERROR_FOREGROUND: u32 = 0xfffffff5;
const WORKSPACE_ERROR_MUTED: u32 = 0xffffffb8;
const FROST_SAMPLE_OFFSETS: [(f32, f32); 5] = [
    (0.0, 0.0),
    (-5.0, -3.0),
    (5.0, 3.0),
    (-3.0, 5.0),
    (3.0, -5.0),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WorkspaceErrorBannerColors {
    background: u32,
    foreground: u32,
    muted: u32,
}

fn workspace_error_banner_colors(theme: Theme) -> WorkspaceErrorBannerColors {
    WorkspaceErrorBannerColors {
        background: with_alpha(theme.graphite, 0xf0),
        foreground: WORKSPACE_ERROR_FOREGROUND,
        muted: WORKSPACE_ERROR_MUTED,
    }
}

/// Incremental source-over alpha that keeps every accumulated spatial sample
/// equally weighted. Starting with an opaque first sample prevents the card's
/// cool-white shell from replacing wallpaper energy; sample `n` then mixes at
/// `1 / (n + 1)`, leaving all accumulated samples with the same final weight.
fn frost_sample_opacity(sample_index: usize) -> f32 {
    1.0 / (sample_index as f32 + 1.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FrostedBackdropRegion {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
}

/// GPUI 0.2.2 只能矩形裁剪子元素。将毛玻璃拆为中间横条与上下内缩横条，
/// 让四个圆角方块继续显示底下同位置的原始壁纸，而不被矩形采样盖住。
fn frosted_backdrop_regions(
    card_width: f32,
    card_height: f32,
    card_radius: f32,
) -> [FrostedBackdropRegion; 3] {
    let width = card_width.max(0.0);
    let height = card_height.max(0.0);
    let radius = card_radius.max(0.0).min(width * 0.5).min(height * 0.5);
    let inner_width = (width - radius * 2.0).max(0.0);
    let inner_height = (height - radius * 2.0).max(0.0);

    [
        FrostedBackdropRegion {
            left: radius,
            top: 0.0,
            width: inner_width,
            height: radius,
        },
        FrostedBackdropRegion {
            left: 0.0,
            top: radius,
            width,
            height: inner_height,
        },
        FrostedBackdropRegion {
            left: radius,
            top: height - radius,
            width: inner_width,
            height: radius,
        },
    ]
}

impl MultisoulApp {
    pub(super) fn render_workspace_error_banner(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let error = self.workspace_state.error.clone()?;
        let theme = self.theme;
        let colors = workspace_error_banner_colors(theme);
        Some(
            div()
                .id("workspace-error-layer")
                .absolute()
                .top(px(66.0))
                .left(px(0.0))
                .right(px(0.0))
                .flex()
                .justify_center()
                .child(
                    div()
                        .id("workspace-error-banner")
                        .debug_selector(|| "workspace-error-banner".to_string())
                        .mx(px(24.0))
                        .w_full()
                        .max_w(px(720.0))
                        .min_h(px(42.0))
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .px(px(14.0))
                        .py(px(10.0))
                        .rounded(px(12.0))
                        .border_1()
                        .border_color(rgba(with_alpha(theme.destructive, 0x70)))
                        .bg(rgba(colors.background))
                        .text_color(rgba(colors.foreground))
                        .text_size(px(12.0))
                        .child(
                            div()
                                .id("workspace-error-message")
                                .debug_selector(|| "workspace-error-message".to_string())
                                .min_w(px(0.0))
                                .flex_1()
                                .overflow_hidden()
                                .whitespace_normal()
                                .child(error),
                        )
                        .child(
                            div()
                                .id("workspace-error-dismiss")
                                .debug_selector(|| "workspace-error-dismiss".to_string())
                                .flex_none()
                                .w(px(28.0))
                                .h(px(28.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(8.0))
                                .cursor_pointer()
                                .text_color(rgba(colors.muted))
                                .hover(|style| style.bg(rgba(with_alpha(theme.destructive, 0x24))))
                                .child("×")
                                .on_click(cx.listener(|app, _, _window, cx| {
                                    app.workspace_state.error = None;
                                    cx.notify();
                                })),
                        ),
                ),
        )
    }

    pub(super) fn render_workspace_backdrop(&self) -> Stateful<Div> {
        let theme = self.theme;
        let backdrop = div()
            .id("workspace-backdrop")
            .debug_selector(|| "workspace-backdrop".to_string())
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

        backdrop.child(self.render_wallpaper_layer())
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
    /// five nearby samples with normalized incremental weights. This mirrors a
    /// CSS backdrop blur's spatial mixing without reading pixels back to the
    /// CPU, while keeping the blurred wallpaper fully present beneath the tint.
    /// Static/GIF samples reuse GPUI's image cache; video samples reuse the
    /// current 420f CVPixelBuffer surface.
    pub(crate) fn render_card_frosted_backdrop(
        &self,
        node_id: canvas::NodeId,
        card_left: f32,
        card_top: f32,
        card_width: f32,
        card_height: f32,
        card_radius: f32,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Div {
        let mut backdrop = div()
            .absolute()
            .top(px(card_top))
            .left(px(card_left))
            .w(px(card_width))
            .h(px(card_height))
            .overflow_hidden();

        for region in frosted_backdrop_regions(card_width, card_height, card_radius) {
            backdrop = backdrop.child(self.render_frosted_backdrop_region(
                node_id,
                card_left,
                card_top,
                region,
                viewport_width,
                viewport_height,
            ));
        }

        backdrop
    }

    fn render_frosted_backdrop_region(
        &self,
        node_id: canvas::NodeId,
        card_left: f32,
        card_top: f32,
        region: FrostedBackdropRegion,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Div {
        let kind = self.wallpaper_settings.current_kind();
        let source = self.wallpaper_settings.image_source_for_render();
        let render_key = self.wallpaper_settings.render_id_key();
        let mut patch = div()
            .absolute()
            .left(px(region.left))
            .top(px(region.top))
            .w(px(region.width))
            .h(px(region.height))
            .overflow_hidden();

        for (sample_index, (dx, dy)) in FROST_SAMPLE_OFFSETS.into_iter().enumerate() {
            let sample_left = -card_left + dx - region.left;
            let sample_top = -card_top + dy - region.top;
            let sample_opacity = frost_sample_opacity(sample_index);

            #[cfg(target_os = "macos")]
            if kind == WallpaperKind::Video {
                if let Some(sample) = self.render_video_wallpaper_frost_sample(
                    sample_left,
                    sample_top,
                    viewport_width,
                    viewport_height,
                    sample_opacity,
                ) {
                    patch = patch.child(sample);
                    continue;
                }
            }

            patch = patch.child(
                img(source.clone())
                    .id(gpui::SharedString::from(format!(
                        "card-frost:{render_key}:{node_id}:{sample_index}:{}:{}",
                        region.left, region.top
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

        patch.child(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .bg(linear_gradient(
                    155.0,
                    linear_color_stop(rgba(with_alpha(self.theme.title_bar, 0x24)), 0.0),
                    linear_color_stop(rgba(with_alpha(self.theme.node_fill, 0x2e)), 1.0),
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
            .when(self.voice_ready_notification_visible, |chrome| {
                chrome.child(self.render_voice_ready_notification(cx))
            })
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

#[cfg(test)]
#[path = "../../tests/unit/workspace_backdrop_tests.rs"]
mod workspace_backdrop_tests;

#[cfg(test)]
#[path = "../../tests/unit/workspace_error_banner_tests.rs"]
mod workspace_error_banner_tests;
