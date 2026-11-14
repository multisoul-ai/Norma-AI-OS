//! 幕僚图片走既有聊天上传：PNG/JPEG、2 MiB、边长 2048、每条最多 4 张，写入 `.mobile-images`。
use super::*;
use gpui::{div, img, prelude::*, px, Context, ObjectFit};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use ui::component::{
    button::{Button, ButtonVariants},
    Disableable, IconName, Sizable,
};
use ui::i18n::text as t;

const IMAGE_LIMIT: u64 = 2 * 1024 * 1024;

pub(super) fn staff_can_send(busy: bool, uploading: bool, text: &str, images: usize) -> bool {
    !busy && !uploading && (!text.trim().is_empty() || images > 0)
}

pub(super) fn staff_outgoing(
    text: &str,
    images: &[String],
) -> Result<(String, String), &'static str> {
    let visible = text.trim();
    if images.is_empty() {
        if visible.is_empty() {
            return Err("empty");
        }
        return Ok((visible.to_owned(), title_from(visible, images)));
    }
    if images.len() > norma::protocol::MAX_CHAT_IMAGES {
        return Err("too_many_images");
    }
    Ok((
        norma::protocol::compose_image_message(visible, images)?,
        title_from(visible, images),
    ))
}

fn title_from(visible: &str, images: &[String]) -> String {
    let named = images
        .first()
        .and_then(|path| Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("image");
    let source = visible
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .unwrap_or(named);
    source.chars().take(60).collect()
}

pub(super) fn store_picked_image(
    database: &Path,
    workspace: &str,
    path: &Path,
) -> Result<String, &'static str> {
    let meta = fs::symlink_metadata(path).map_err(|_| "invalid_image")?;
    if !meta.file_type().is_file() || meta.len() == 0 || meta.len() > IMAGE_LIMIT {
        return Err("invalid_image");
    }
    let file = fs::File::open(path).map_err(|_| "invalid_image")?;
    let mut bytes = Vec::new();
    file.take(IMAGE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "invalid_image")?;
    if bytes.is_empty() || bytes.len() as u64 > IMAGE_LIMIT {
        return Err("invalid_image");
    }
    crate::mobile::accept_chat_image(database, workspace, &bytes)
}

const EDGE: u32 = 2048;

/// 剪贴板同时有文件名和图片时收图片。纯文字返回 None，交给输入框。
pub(super) fn staff_image_from_clipboard(
    system: Option<gpui::Image>,
    item: Option<&gpui::ClipboardItem>,
) -> Option<gpui::Image> {
    system.or_else(|| item.and_then(crate::app::clipboard_image::image_from_clipboard_item))
}

pub(super) fn prepare_staff_clipboard_image(image: &gpui::Image) -> Result<Vec<u8>, &'static str> {
    if image.bytes().is_empty() {
        return Err("invalid_image");
    }
    let format = match image.format() {
        gpui::ImageFormat::Png => image::ImageFormat::Png,
        gpui::ImageFormat::Jpeg => image::ImageFormat::Jpeg,
        gpui::ImageFormat::Webp => image::ImageFormat::WebP,
        gpui::ImageFormat::Gif => image::ImageFormat::Gif,
        gpui::ImageFormat::Bmp => image::ImageFormat::Bmp,
        gpui::ImageFormat::Tiff => image::ImageFormat::Tiff,
        gpui::ImageFormat::Svg => return Err("invalid_image"),
    };
    let decoded =
        image::load_from_memory_with_format(image.bytes(), format).map_err(|_| "invalid_image")?;
    if decoded.width() == 0 || decoded.height() == 0 {
        return Err("invalid_image");
    }
    let fits = decoded.width() <= EDGE
        && decoded.height() <= EDGE
        && image.bytes().len() <= IMAGE_LIMIT as usize;
    if fits
        && matches!(
            image.format(),
            gpui::ImageFormat::Png | gpui::ImageFormat::Jpeg
        )
    {
        return Ok(image.bytes().to_vec());
    }
    encode_staff_image(&fit_staff_image(decoded))
}

fn fit_staff_image(image: image::DynamicImage) -> image::DynamicImage {
    let (width, height) = (image.width(), image.height());
    if width <= EDGE && height <= EDGE {
        return image;
    }
    let scale = (EDGE as f32 / width as f32).min(EDGE as f32 / height as f32);
    let fitted_width = ((width as f32) * scale).floor().max(1.0) as u32;
    let fitted_height = ((height as f32) * scale).floor().max(1.0) as u32;
    image.resize(
        fitted_width.min(EDGE),
        fitted_height.min(EDGE),
        image::imageops::FilterType::Triangle,
    )
}

fn encode_staff_image(image: &image::DynamicImage) -> Result<Vec<u8>, &'static str> {
    let mut png = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|_| "invalid_image")?;
    if png.len() <= IMAGE_LIMIT as usize {
        return Ok(png);
    }
    let rgb = image.to_rgb8();
    for quality in [85, 70, 50, 30] {
        let mut jpeg = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
            std::io::Cursor::new(&mut jpeg),
            quality,
        );
        if encoder.encode_image(&rgb).is_ok()
            && jpeg.len() <= IMAGE_LIMIT as usize
            && !jpeg.is_empty()
        {
            return Ok(jpeg);
        }
    }
    Err("invalid_image")
}

impl MultisoulApp {
    pub(super) fn stash_staff_images(&mut self, next: Option<String>) {
        let current = self.staff.selected.clone().unwrap_or_default();
        let next = next.unwrap_or_default();
        if current == next {
            return;
        }
        self.staff
            .image_drafts
            .insert(current, std::mem::take(&mut self.staff.images));
        self.staff.images = self.staff.image_drafts.remove(&next).unwrap_or_default();
    }

    pub(super) fn staff_image_chips(&self, cx: &mut Context<Self>) -> gpui::Div {
        let mut row = div().flex().flex_wrap().gap(px(10.)).pb(px(8.));
        for (index, path) in self.staff.images.iter().cloned().enumerate() {
            row = row.child(
                div()
                    .id(("staff-image", index))
                    .debug_selector(move || format!("staff-image-{index}"))
                    .w(px(104.))
                    .h(px(78.))
                    .child(
                        img(PathBuf::from(path))
                            .size_full()
                            .rounded(px(10.))
                            .object_fit(ObjectFit::Cover),
                    )
                    .child(
                        div().absolute().top(px(4.)).right(px(4.)).child(
                            Button::new(("staff-image-remove", index))
                                .debug_selector(move || format!("staff-image-remove-{index}"))
                                .custom(self.staff_image_remove_style(cx))
                                .with_size(px(18.))
                                .rounded_full()
                                .icon(IconName::Close)
                                .tooltip(t("staff.remove_image"))
                                .disabled(self.staff.busy || self.staff.image_uploads > 0)
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    app.remove_staff_image(index, cx);
                                })),
                        ),
                    ),
            );
        }
        row
    }

    /// 有图片时吃掉粘贴，缩进既有上传限制后收成附件。没有图片时返回 false，文字照常进输入框。
    pub(super) fn paste_staff_clipboard_image(&mut self, cx: &mut Context<Self>) -> bool {
        let item = cx.read_from_clipboard();
        let Some(image) = staff_image_from_clipboard(
            crate::app::clipboard_image::read_clipboard_image_first(None),
            item.as_ref(),
        ) else {
            return false;
        };
        if self.staff.busy {
            return true;
        }
        if self.staff.images.len() + usize::from(self.staff.image_uploads)
            >= norma::protocol::MAX_CHAT_IMAGES
        {
            self.staff.error = Some(super::error_message("too_many_images".into()));
            self.notify_staff(cx);
            return true;
        }
        if let Err(reason) = self.staff_ready() {
            self.staff.error = Some(super::error_message(reason.to_owned()));
            self.notify_staff(cx);
            return true;
        }
        let Some(workspace) = self.workspace_id.clone() else {
            self.staff.error = Some(super::error_message("island.staff_no_workspace".into()));
            self.notify_staff(cx);
            return true;
        };
        let database = self.database_paths.app_db_path().to_path_buf();
        let draft_key = self.staff.selected.clone().unwrap_or_default();
        self.staff.image_uploads = self.staff.image_uploads.saturating_add(1);
        self.staff.error = None;
        self.notify_staff(cx);
        cx.spawn(async move |app, cx| {
            let saved = cx
                .background_executor()
                .spawn(async move {
                    let bytes = prepare_staff_clipboard_image(&image)?;
                    crate::mobile::accept_chat_image(&database, &workspace, &bytes)
                })
                .await;
            let _ = app.update(cx, |app, cx| {
                app.staff.image_uploads = app.staff.image_uploads.saturating_sub(1);
                match saved {
                    Ok(path) => app.attach_staff_image(draft_key, path),
                    Err(error) => {
                        app.staff.error = Some(super::error_message(error.to_owned()));
                    }
                }
                app.notify_staff(cx);
            });
        })
        .detach();
        true
    }

    pub(super) fn pick_staff_image(&mut self, cx: &mut Context<Self>) {
        if self.staff.busy || self.staff.image_uploads > 0 {
            return;
        }
        if self.staff.images.len() >= norma::protocol::MAX_CHAT_IMAGES {
            self.staff.error = Some(super::error_message("too_many_images".into()));
            self.notify_staff(cx);
            return;
        }
        if let Err(reason) = self.staff_ready() {
            self.staff.error = Some(super::error_message(reason.to_owned()));
            self.notify_staff(cx);
            return;
        }
        let Some(workspace) = self.workspace_id.clone() else {
            self.staff.error = Some(super::error_message("island.staff_no_workspace".into()));
            self.notify_staff(cx);
            return;
        };
        let database = self.database_paths.app_db_path().to_path_buf();
        let draft_key = self.staff.selected.clone().unwrap_or_default();
        let title = t("staff.attach_image").to_owned();
        self.staff.image_uploads = self.staff.image_uploads.saturating_add(1);
        self.staff.error = None;
        self.notify_staff(cx);
        cx.spawn(async move |app, cx| {
            let picked = crate::app::file_picker::pick_chat_image(&title).await;
            let saved = match picked {
                Some(path) => {
                    cx.background_executor()
                        .spawn(async move { store_picked_image(&database, &workspace, &path) })
                        .await
                }
                None => Err("cancelled"),
            };
            let _ = app.update(cx, |app, cx| {
                app.staff.image_uploads = app.staff.image_uploads.saturating_sub(1);
                match saved {
                    Ok(path) => app.attach_staff_image(draft_key, path),
                    Err("cancelled") => {}
                    Err(error) => {
                        app.staff.error = Some(super::error_message(error.to_owned()));
                    }
                }
                app.notify_staff(cx);
            });
        })
        .detach();
    }

    fn attach_staff_image(&mut self, draft_key: String, path: String) {
        let current = self.staff.selected.clone().unwrap_or_default();
        let slot = if current == draft_key {
            &mut self.staff.images
        } else {
            self.staff.image_drafts.entry(draft_key).or_default()
        };
        if slot.len() >= norma::protocol::MAX_CHAT_IMAGES {
            self.staff.error = Some(super::error_message("too_many_images".into()));
            return;
        }
        if slot.iter().all(|old| old != &path) {
            slot.push(path);
        }
    }

    pub(super) fn remove_staff_image(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.staff.images.len() {
            return;
        }
        let path = self.staff.images.remove(index);
        if !self.staff_image_referenced(&path) {
            cx.background_executor()
                .spawn(async move {
                    let _ = fs::remove_file(path);
                })
                .detach();
        }
        self.notify_staff(cx);
    }

    fn staff_image_referenced(&self, path: &str) -> bool {
        self.staff.images.iter().any(|item| item == path)
            || self
                .staff
                .image_drafts
                .values()
                .any(|items| items.iter().any(|item| item == path))
            || self.staff.tasks.values().any(|task| {
                task.messages.iter().any(|message| match message {
                    norma::protocol::TaskMessage::User { text } => text.contains(path),
                    _ => false,
                })
            })
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/staff_image_tests.rs"]
mod tests;
