//! A wallpaper-aligned frosted layer for the two workspace drawers.
//!
//! GPUI 0.2.2 has window blur but no element backdrop filter. Decode one bounded
//! still frame off the UI thread, blur it once, and crop it behind each drawer
//! at the same viewport coordinates as the wallpaper.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{
    div, img, linear_color_stop, linear_gradient, prelude::*, px, rgba, Context, Div, ImageSource,
    ObjectFit, RenderImage, Stateful, Window,
};
use image::{DynamicImage, Frame, ImageReader, RgbaImage};

use crate::app::wallpaper_resolvers::WallpaperKind;
use crate::app::MultisoulApp;
use crate::settings::wallpaper::state::{
    is_embedded_default, resolve_selection_path, WallpaperSelection,
};

const FROST_MAX_EDGE_PX: u32 = 1280;
const FROST_SIGMA: f32 = 24.0;

pub(super) fn drawer_tint(theme: ui::Theme, _left: bool) -> (u32, u32) {
    // 60% tint (round(255 * 0.60)) lets more blurred wallpaper show through both drawers.
    (
        ui::icons::with_alpha(theme.canvas_from, 0x99),
        ui::icons::with_alpha(theme.canvas_to, 0x99),
    )
}

#[derive(Default)]
pub(crate) struct FrostedWallpaperState {
    selection: Option<WallpaperSelection>,
    generation: u64,
    image: Option<Arc<RenderImage>>,
}

impl FrostedWallpaperState {
    pub(super) fn take_selection_update(
        &mut self,
        selection: &WallpaperSelection,
    ) -> Option<(u64, WallpaperSelection)> {
        if self.selection.as_ref() == Some(selection) {
            return None;
        }
        self.selection = Some(selection.clone());
        self.generation = self.generation.wrapping_add(1);
        Some((self.generation, selection.clone()))
    }

    pub(crate) fn invalidate_for_builtin(&mut self, rel: &str) -> bool {
        if !matches!(self.selection.as_ref(), Some(WallpaperSelection::Builtin { rel: selected }) if *selected == rel)
        {
            return false;
        }
        self.invalidate()
    }

    pub(crate) fn invalidate(&mut self) -> bool {
        if self.selection.is_none() {
            return false;
        }
        self.selection = None;
        self.generation = self.generation.wrapping_add(1);
        true
    }
}

pub(super) fn frost_wallpaper_pixels(source: &RgbaImage) -> RgbaImage {
    image::imageops::blur(source, FROST_SIGMA)
}

pub(super) fn effective_frost_source(
    selected: Option<PathBuf>,
    kind: WallpaperKind,
) -> Option<(PathBuf, WallpaperKind)> {
    selected.map(|path| (path, kind)).or_else(|| {
        crate::app::wallpaper_resolvers::resolve_builtin_asset(
            crate::assets::DEFAULT_WALLPAPER_ASSET,
        )
        .map(|path| (path, WallpaperKind::Static))
    })
}

fn decode_frosted_wallpaper(path: &Path, kind: WallpaperKind) -> anyhow::Result<Arc<RenderImage>> {
    let rgba = match kind {
        WallpaperKind::Static | WallpaperKind::Gif => ImageReader::open(path)?
            .with_guessed_format()?
            .decode()?
            .into_rgba8(),
        WallpaperKind::Video => {
            #[cfg(target_os = "macos")]
            {
                crate::app::video_decoder::VideoDecoder::first_frame_bgra(path)?
            }
            #[cfg(not(target_os = "macos"))]
            {
                anyhow::bail!("video wallpaper frost requires macOS")
            }
        }
    };
    Ok(frost_rgba(rgba))
}

fn frost_rgba(rgba: RgbaImage) -> Arc<RenderImage> {
    let bounded = if rgba.width() > FROST_MAX_EDGE_PX || rgba.height() > FROST_MAX_EDGE_PX {
        DynamicImage::ImageRgba8(rgba)
            .thumbnail(FROST_MAX_EDGE_PX, FROST_MAX_EDGE_PX)
            .into_rgba8()
    } else {
        rgba
    };
    let mut frosted = frost_wallpaper_pixels(&bounded);
    for pixel in frosted.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Arc::new(RenderImage::new(vec![Frame::new(frosted)]))
}

/// Decodes the compiled-in default wallpaper. It never touches the file system, so an
/// unbundled build cannot read the source checkout (which may sit in a TCC-protected folder).
fn decode_frosted_embedded_default() -> anyhow::Result<(PathBuf, Arc<RenderImage>)> {
    use gpui::AssetSource;
    let name = crate::assets::DEFAULT_WALLPAPER_ASSET;
    let bytes = crate::assets::EmbeddedAssets
        .load(name)?
        .ok_or_else(|| anyhow::anyhow!("embedded wallpaper {name} is missing"))?;
    let rgba = image::load_from_memory(&bytes)?.into_rgba8();
    Ok((PathBuf::from(name), frost_rgba(rgba)))
}

pub(super) fn decode_frosted_selection(
    selection: &WallpaperSelection,
    kind: WallpaperKind,
) -> anyhow::Result<(PathBuf, Arc<RenderImage>)> {
    if is_embedded_default(selection) {
        return decode_frosted_embedded_default();
    }
    decode_frosted_source(resolve_selection_path(selection), kind)
}

pub(super) fn decode_frosted_source(
    selected: Option<PathBuf>,
    kind: WallpaperKind,
) -> anyhow::Result<(PathBuf, Arc<RenderImage>)> {
    let (source, kind) = effective_frost_source(selected, kind)
        .ok_or_else(|| anyhow::anyhow!("default wallpaper source is unavailable"))?;
    match decode_frosted_wallpaper(&source, kind) {
        Ok(image) => Ok((source, image)),
        Err(error) => {
            let fallback = crate::app::wallpaper_resolvers::resolve_builtin_asset(
                crate::assets::DEFAULT_WALLPAPER_ASSET,
            )
            .filter(|fallback| *fallback != source)
            .ok_or(error)?;
            let image = decode_frosted_wallpaper(&fallback, WallpaperKind::Static)?;
            Ok((fallback, image))
        }
    }
}

impl MultisoulApp {
    pub(super) fn ensure_drawer_frost(&mut self, cx: &mut Context<Self>) {
        let Some((generation, selection)) = self
            .desktop_workspace
            .frosted_wallpaper
            .take_selection_update(&self.wallpaper_settings.selection)
        else {
            return;
        };
        let kind = self.wallpaper_settings.current_kind();
        cx.spawn(async move |this, cx| {
            let decode_selection = selection.clone();
            let result = cx
                .background_executor()
                .spawn(async move { decode_frosted_selection(&decode_selection, kind) })
                .await;
            let _ = this.update(cx, |app, cx| {
                let state = &mut app.desktop_workspace.frosted_wallpaper;
                if state.generation != generation || state.selection.as_ref() != Some(&selection) {
                    if let Ok((_, image)) = result {
                        cx.defer(move |cx| cx.drop_image(image, None));
                    }
                    return;
                }
                match result {
                    Ok((_, image)) => {
                        let previous = state.image.replace(image);
                        if let Some(previous) = previous {
                            cx.defer(move |cx| cx.drop_image(previous, None));
                        }
                        cx.notify();
                    }
                    Err(error) => {
                        if let Some(previous) = state.image.take() {
                            cx.defer(move |cx| cx.drop_image(previous, None));
                        }
                        tracing::warn!(
                            %error,
                            selection = ?selection,
                            "drawer wallpaper blur failed"
                        );
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    pub(super) fn render_drawer_frost(
        &self,
        left: bool,
        width: f32,
        edge_inset: f32,
        window: &Window,
    ) -> Stateful<Div> {
        let viewport = window.viewport_size();
        let viewport_width = f32::from(viewport.width);
        let viewport_height = f32::from(viewport.height);
        let panel_x = if left {
            edge_inset
        } else {
            viewport_width - width - edge_inset
        };
        // Keep enough tint for legible controls while letting the blurred
        // Earth Horizon colors read through both drawer surfaces.
        let tint = drawer_tint(self.desktop_theme(), left);
        div()
            .id(if left {
                "desktop-assistant-left-frost"
            } else {
                "desktop-assistant-right-frost"
            })
            .debug_selector(move || {
                if left {
                    "desktop-assistant-left-frost".to_string()
                } else {
                    "desktop-assistant-right-frost".to_string()
                }
            })
            .absolute()
            .inset_0()
            .overflow_hidden()
            .when_some(
                self.desktop_workspace.frosted_wallpaper.image.clone(),
                |layer, image| {
                    layer.child(
                        img(ImageSource::Render(image))
                            .absolute()
                            .left(px(-panel_x))
                            .top(px(0.0))
                            .w(px(viewport_width))
                            .h(px(viewport_height))
                            .object_fit(ObjectFit::Cover),
                    )
                },
            )
            .when_some(
                self.desktop_theme().wallpaper_veil(),
                |layer, (from, to)| {
                    layer.child(div().absolute().inset_0().bg(linear_gradient(
                        155.0,
                        linear_color_stop(rgba(from), 0.0),
                        linear_color_stop(rgba(to), 1.0),
                    )))
                },
            )
            .child(div().absolute().inset_0().bg(linear_gradient(
                155.0,
                linear_color_stop(rgba(tint.0), 0.0),
                linear_color_stop(rgba(tint.1), 1.0),
            )))
            .child(div().absolute().inset_0().bg(linear_gradient(
                145.0,
                linear_color_stop(
                    rgba(ui::icons::with_alpha(
                        self.desktop_theme().text_primary,
                        0x05,
                    )),
                    0.0,
                ),
                linear_color_stop(rgba(0x00000000), 0.6),
            )))
    }
}
