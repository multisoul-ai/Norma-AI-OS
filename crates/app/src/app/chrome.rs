//! Workspace overlay chrome for the GPUI app shell.

mod dock;
mod top_bar;

use super::MultisoulApp;
use crate::app::wallpaper_resolvers::WallpaperKind;
use gpui::{
    div, img, linear_color_stop, linear_gradient, point, prelude::*, px, rgba, BoxShadow, Context,
    Div, ObjectFit, Stateful, Window,
};
use ui::icons::with_alpha;
use ui::Theme;

const WORKSPACE_LEFT_RAIL_VISIBLE: bool = false;
const BOTTOM_VOICE_BAR_VISIBLE: bool = true;
pub(super) const WALLPAPER_IMAGE_OPACITY: f32 = 1.0;
const WORKSPACE_ERROR_FOREGROUND: u32 = 0xfffffff5;
const WORKSPACE_ERROR_MUTED: u32 = 0xffffffb8;
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

    pub(crate) fn render_workspace_backdrop(&self) -> Stateful<Div> {
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

    pub(super) fn render_workspace_chrome(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.theme;
        let voice_caption = self.render_voice_caption(cx);

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .when_some(voice_caption, |chrome, caption| chrome.child(caption))
            .child(self.render_command_dock(window, cx, None))
            .child(self.render_workspace_top_bar(cx))
            .when_some(
                self.render_command_suggestions(window, cx),
                |chrome, menu| chrome.child(menu),
            )
            .when(self.voice_ready_notification_visible, |chrome| {
                chrome.child(self.render_voice_ready_notification(cx))
            })
            .when(self.presentation_notification.is_some(), |chrome| {
                chrome.child(self.render_presentation_notification(cx))
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
            .when(self.interaction.layout_menu_open, |chrome| {
                chrome.child(self.render_layout_menu(cx))
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
#[path = "../../tests/unit/product_ui_chrome_tests.rs"]
mod product_ui_chrome_tests;

#[cfg(test)]
#[path = "../../tests/unit/workspace_backdrop_tests.rs"]
mod workspace_backdrop_tests;

#[cfg(test)]
#[path = "../../tests/unit/workspace_error_banner_tests.rs"]
mod workspace_error_banner_tests;
