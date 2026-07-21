//! Bounded, single-frame previews for the wallpaper settings grid.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use anyhow::{Context as _, Result};
use gpui::{Context, RenderImage};
use image::{DynamicImage, Frame, ImageReader, RgbaImage};
use tokio::sync::Semaphore;

use crate::app::wallpaper_resolvers::WallpaperKind;
use crate::app::MultisoulApp;

/// The settings card never needs more source pixels than a 2× thumbnail.
pub(crate) const PREVIEW_MAX_EDGE_PX: u32 = 640;
pub(crate) const PREVIEW_CACHE_BUDGET_BYTES: usize = 64 * 1024 * 1024;
const MAX_CONCURRENT_DECODES: usize = 2;

pub(crate) struct WallpaperPreview {
    pub(crate) image: Arc<RenderImage>,
    pub(crate) byte_len: usize,
}

pub(crate) struct PreviewCache {
    ready: HashMap<PathBuf, WallpaperPreview>,
    inflight: HashSet<PathBuf>,
    least_to_most_recent: VecDeque<PathBuf>,
    total_bytes: usize,
    budget_bytes: usize,
    generation: Arc<AtomicU64>,
}

impl Default for PreviewCache {
    fn default() -> Self {
        Self::with_budget(PREVIEW_CACHE_BUDGET_BYTES)
    }
}

impl PreviewCache {
    pub(crate) fn with_budget(budget_bytes: usize) -> Self {
        Self {
            ready: HashMap::new(),
            inflight: HashSet::new(),
            least_to_most_recent: VecDeque::new(),
            total_bytes: 0,
            budget_bytes,
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    pub(crate) fn get(&mut self, path: &Path) -> Option<Arc<RenderImage>> {
        let image = self.ready.get(path)?.image.clone();
        self.touch(path);
        Some(image)
    }

    pub(crate) fn begin_request(&mut self, path: PathBuf) -> Option<u64> {
        if self.ready.contains_key(&path) || !self.inflight.insert(path) {
            return None;
        }
        Some(self.generation.load(Ordering::Acquire))
    }

    pub(crate) fn complete_request(
        &mut self,
        path: PathBuf,
        generation: u64,
        preview: WallpaperPreview,
    ) -> Vec<Arc<RenderImage>> {
        if generation != self.generation.load(Ordering::Acquire) || !self.inflight.remove(&path) {
            return vec![preview.image];
        }
        self.total_bytes += preview.byte_len;
        self.ready.insert(path.clone(), preview);
        self.touch(&path);

        let mut evicted = Vec::new();
        while self.total_bytes > self.budget_bytes {
            let Some(oldest) = self.least_to_most_recent.pop_front() else {
                break;
            };
            if let Some(preview) = self.ready.remove(&oldest) {
                self.total_bytes = self.total_bytes.saturating_sub(preview.byte_len);
                evicted.push(preview.image);
            }
        }
        evicted
    }

    pub(crate) fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    pub(crate) fn len(&self) -> usize {
        self.ready.len()
    }

    pub(crate) fn clear(&mut self) -> Vec<Arc<RenderImage>> {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.inflight.clear();
        self.least_to_most_recent.clear();
        self.total_bytes = 0;
        self.ready
            .drain()
            .map(|(_, preview)| preview.image)
            .collect()
    }

    fn fail_request(&mut self, path: &Path, generation: u64) {
        if generation == self.generation.load(Ordering::Acquire) {
            self.inflight.remove(path);
        }
    }

    fn touch(&mut self, path: &Path) {
        self.least_to_most_recent.retain(|entry| entry != path);
        self.least_to_most_recent.push_back(path.to_path_buf());
    }
}

/// Convert RGBA pixels into one bounded GPUI BGRA frame.
pub(crate) fn render_preview_from_rgba(rgba: RgbaImage) -> WallpaperPreview {
    let mut buffer = if rgba.width() > PREVIEW_MAX_EDGE_PX || rgba.height() > PREVIEW_MAX_EDGE_PX {
        DynamicImage::ImageRgba8(rgba)
            .thumbnail(PREVIEW_MAX_EDGE_PX, PREVIEW_MAX_EDGE_PX)
            .into_rgba8()
    } else {
        rgba
    };
    for pixel in buffer.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    let byte_len = buffer.len();
    let image = Arc::new(RenderImage::new(vec![Frame::new(buffer)]));
    WallpaperPreview { image, byte_len }
}

/// Decode one still frame from an image file and bound it for the settings grid.
/// `ImageReader::decode` intentionally returns one `DynamicImage`, so animated
/// formats never enter GPUI as a multi-frame `RenderImage`.
pub(crate) fn decode_image_preview(path: &Path) -> Result<WallpaperPreview> {
    let rgba = ImageReader::open(path)
        .with_context(|| format!("open wallpaper preview {}", path.display()))?
        .with_guessed_format()
        .with_context(|| format!("detect wallpaper preview format {}", path.display()))?
        .decode()
        .with_context(|| format!("decode wallpaper preview {}", path.display()))?
        .into_rgba8();
    Ok(render_preview_from_rgba(rgba))
}

/// Start one deduplicated preview decode. At most two source files are decoded
/// concurrently, so opening the 16-card grid cannot expand every original at once.
pub(crate) fn request(
    kind: WallpaperKind,
    path: PathBuf,
    cache: &mut PreviewCache,
    cx: &mut Context<MultisoulApp>,
) {
    let Some(generation) = cache.begin_request(path.clone()) else {
        return;
    };
    let generation_token = cache.generation.clone();
    cx.spawn(async move |this, cx| {
        let decode_path = path.clone();
        let result = cx
            .background_executor()
            .spawn(async move {
                decode_source_preview(kind, &decode_path, generation, generation_token).await
            })
            .await;
        let _ = this.update(cx, |app, cx| match result {
            Ok(Some(preview)) => {
                let released = app
                    .wallpaper_previews
                    .complete_request(path, generation, preview);
                release_images(released, cx);
                cx.notify();
            }
            Ok(None) => {}
            Err(error) => {
                app.wallpaper_previews.fail_request(&path, generation);
                tracing::warn!(%error, source = %path.display(), "wallpaper preview decode failed");
            }
        });
    })
    .detach();
}

pub(crate) fn release_images(images: Vec<Arc<RenderImage>>, cx: &mut Context<MultisoulApp>) {
    if images.is_empty() {
        return;
    }
    cx.defer(move |cx| {
        for image in images {
            cx.drop_image(image, None);
        }
    });
}

async fn decode_source_preview(
    kind: WallpaperKind,
    path: &Path,
    generation: u64,
    generation_token: Arc<AtomicU64>,
) -> Result<Option<WallpaperPreview>> {
    let permit = decode_semaphore()
        .clone()
        .acquire_owned()
        .await
        .context("wallpaper preview decode semaphore closed")?;
    if generation_token.load(Ordering::Acquire) != generation {
        return Ok(None);
    }
    let preview = match kind {
        WallpaperKind::Static | WallpaperKind::Gif => decode_image_preview(path)?,
        WallpaperKind::Video => decode_video_preview(path)?,
    };
    drop(permit);
    Ok(Some(preview))
}

#[cfg(target_os = "macos")]
fn decode_video_preview(path: &Path) -> Result<WallpaperPreview> {
    let rgba = crate::app::video_decoder::VideoDecoder::first_frame_bgra(path)?;
    Ok(render_preview_from_rgba(rgba))
}

#[cfg(not(target_os = "macos"))]
fn decode_video_preview(_path: &Path) -> Result<WallpaperPreview> {
    anyhow::bail!("video wallpaper previews require macOS")
}

fn decode_semaphore() -> &'static Arc<Semaphore> {
    static DECODES: OnceLock<Arc<Semaphore>> = OnceLock::new();
    DECODES.get_or_init(|| Arc::new(Semaphore::new(MAX_CONCURRENT_DECODES)))
}
