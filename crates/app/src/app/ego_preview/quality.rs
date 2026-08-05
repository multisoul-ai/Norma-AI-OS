use image::RgbaImage;
use std::collections::HashSet;

pub(super) const EGO_PREVIEW_MIN_COLOR_SCORE: usize = 8;

pub(super) fn validate_ego_preview_rgba(source: &str, rgba: &RgbaImage) -> Result<usize, String> {
    let score = preview_color_score(rgba);
    if score < EGO_PREVIEW_MIN_COLOR_SCORE {
        return Err(format!(
            "{source}未返回可识别的页面内容（量化颜色 {score} < {EGO_PREVIEW_MIN_COLOR_SCORE}）"
        ));
    }
    Ok(score)
}

pub(super) fn preview_color_score(rgba: &RgbaImage) -> usize {
    let width = rgba.width() as usize;
    let height = rgba.height() as usize;
    if width == 0 || height == 0 {
        return 0;
    }
    // 忽略约 1/8 顶部窗口栏；content_pixels ≈ width × height × 7 / 8。
    let content_start = width * (height / 8);
    let content_pixels = width * height - content_start;
    // 最多均匀采样约 2_000 个像素，避免对 4K 窗口做全量颜色集合统计。
    let stride = (content_pixels / 2_000).max(1);
    rgba.pixels()
        .skip(content_start)
        .step_by(stride)
        .map(|pixel| [pixel[0] >> 4, pixel[1] >> 4, pixel[2] >> 4])
        .collect::<HashSet<_>>()
        .len()
}
