//! Clipboard image paste support for terminal-backed Agent cards.

use canvas::{AgentProfile, CanvasNodeKind, NodeId};
use gpui::{ClipboardEntry, ClipboardItem, Image, ImageFormat};
use runtime::Runtime;
use std::fs::{self, DirEntry};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(crate) const CLIPBOARD_IMAGE_DIR_NAME: &str = "multisoul-clipboard-images";
const CLIPBOARD_IMAGE_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const IMAGE_FORMATS_BY_PASTE_PRIORITY: &[ImageFormat] = &[
    ImageFormat::Png,
    ImageFormat::Jpeg,
    ImageFormat::Webp,
    ImageFormat::Gif,
    ImageFormat::Svg,
    ImageFormat::Bmp,
    ImageFormat::Tiff,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SavedClipboardImage {
    pub(crate) path: PathBuf,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClipboardImagePasteMode {
    NativeShortcut,
    PathFallback,
}

pub(crate) fn clipboard_image_paste_mode(
    runtime: &Runtime,
    node_id: NodeId,
) -> Option<ClipboardImagePasteMode> {
    let node = runtime.document.node(node_id)?;
    match &node.kind {
        CanvasNodeKind::Agent(spec)
            if matches!(spec.profile, AgentProfile::Claude | AgentProfile::Codex) =>
        {
            Some(ClipboardImagePasteMode::NativeShortcut)
        }
        CanvasNodeKind::Agent(_) => Some(ClipboardImagePasteMode::PathFallback),
        _ => None,
    }
}

pub(crate) fn clipboard_image_extension(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Webp => "webp",
        ImageFormat::Gif => "gif",
        ImageFormat::Svg => "svg",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Tiff => "tiff",
    }
}

pub(crate) fn image_from_clipboard_item(item: &ClipboardItem) -> Option<Image> {
    image_from_clipboard_entries(item.entries())
}

pub(crate) fn image_from_clipboard_entries(entries: &[ClipboardEntry]) -> Option<Image> {
    entries.iter().find_map(|entry| match entry {
        ClipboardEntry::Image(image) => Some(image.clone()),
        ClipboardEntry::String(_) => None,
    })
}

pub(crate) fn read_clipboard_image_first(item: Option<&ClipboardItem>) -> Option<Image> {
    read_system_clipboard_image().or_else(|| item.and_then(image_from_clipboard_item))
}

pub(crate) fn save_clipboard_image_to_system_temp(
    image: &Image,
) -> io::Result<SavedClipboardImage> {
    let root = std::env::temp_dir();
    let now = SystemTime::now();
    let dir = root.join(CLIPBOARD_IMAGE_DIR_NAME);
    let _ = cleanup_expired_clipboard_images(&dir, now, |entry| entry.metadata()?.modified());
    save_clipboard_image_to_temp_root(&root, image, now)
}

pub(crate) fn save_clipboard_image_to_temp_root(
    temp_root: &Path,
    image: &Image,
    now: SystemTime,
) -> io::Result<SavedClipboardImage> {
    let dir = temp_root.join(CLIPBOARD_IMAGE_DIR_NAME);
    fs::create_dir_all(&dir)?;
    let path = dir.join(clipboard_image_filename(image, now));
    fs::write(&path, image.bytes())?;
    let text = path_to_clipboard_text(&path)?;
    Ok(SavedClipboardImage { path, text })
}

pub(crate) fn cleanup_expired_clipboard_images<F>(
    dir: &Path,
    now: SystemTime,
    mut modified_at: F,
) -> io::Result<usize>
where
    F: FnMut(&DirEntry) -> io::Result<SystemTime>,
{
    if !dir.exists() {
        return Ok(0);
    }

    let mut removed = 0;
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else {
            continue;
        };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let Ok(modified) = modified_at(&entry) else {
            continue;
        };
        if !clipboard_image_is_expired(now, modified) {
            continue;
        }
        match fs::remove_file(entry.path()) {
            Ok(()) => removed += 1,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(removed)
}

fn clipboard_image_filename(image: &Image, now: SystemTime) -> String {
    let since_epoch = now.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
    format!(
        "clipboard-{}-{:09}-{:016x}.{}",
        since_epoch.as_secs(),
        since_epoch.subsec_nanos(),
        image.id(),
        clipboard_image_extension(image.format())
    )
}

fn clipboard_image_is_expired(now: SystemTime, modified: SystemTime) -> bool {
    now.duration_since(modified)
        .is_ok_and(|age| age > CLIPBOARD_IMAGE_MAX_AGE)
}

fn path_to_clipboard_text(path: &Path) -> io::Result<String> {
    path.to_str().map(str::to_owned).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "clipboard image path is not utf-8",
        )
    })
}

#[cfg(not(target_os = "macos"))]
fn read_system_clipboard_image() -> Option<Image> {
    None
}

#[cfg(target_os = "macos")]
fn read_system_clipboard_image() -> Option<Image> {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG, NSPasteboardTypeTIFF};
    use objc2_foundation::NSString;

    let pasteboard = NSPasteboard::generalPasteboard();
    for format in IMAGE_FORMATS_BY_PASTE_PRIORITY {
        let data = match format {
            ImageFormat::Png => pasteboard.dataForType(unsafe { NSPasteboardTypePNG }),
            ImageFormat::Tiff => pasteboard.dataForType(unsafe { NSPasteboardTypeTIFF }),
            ImageFormat::Jpeg => pasteboard.dataForType(&NSString::from_str("public.jpeg")),
            ImageFormat::Webp => {
                pasteboard.dataForType(&NSString::from_str("org.webmproject.webp"))
            }
            ImageFormat::Gif => pasteboard.dataForType(&NSString::from_str("com.compuserve.gif")),
            ImageFormat::Svg => pasteboard.dataForType(&NSString::from_str("public.svg-image")),
            ImageFormat::Bmp => pasteboard.dataForType(&NSString::from_str("com.microsoft.bmp")),
        };
        if let Some(data) = data {
            return Some(Image::from_bytes(*format, data.to_vec()));
        }
    }
    None
}

#[cfg(test)]
#[path = "../../tests/unit/clipboard_image_tests.rs"]
mod tests;
