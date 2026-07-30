use super::ego::{ego_browser_helper, run_helper};
use image::{ImageReader, RgbaImage};
use std::path::{Path, PathBuf};

const EGO_SCREENSHOT_PREFIX: &str = "NORMA_EGO_SCREENSHOT=";

pub(super) fn capture_ego_task_space_rgba(task_space_id: &str) -> Result<RgbaImage, String> {
    let encoded_id = serde_json::to_string(task_space_id)
        .map_err(|error| format!("无法编码 Ego Task Space ID：{error}"))?;
    let script = format!(
        r#"const taskSpaceId = {encoded_id};
await useOrCreateTaskSpace(taskSpaceId);
const screenshotPath = await captureScreenshot();
cliLog("{EGO_SCREENSHOT_PREFIX}" + String(screenshotPath));
"#
    );
    let output = run_helper(&ego_browser_helper(), &script)?;
    let path = parse_ego_screenshot_path(&output)?;
    if !is_owned_ego_temp_screenshot(&path) {
        return Err(format!(
            "ego-browser 返回了不受控的截图路径：{}",
            path.display()
        ));
    }

    let decoded = decode_ego_screenshot(&path);
    let _ = std::fs::remove_file(&path);
    decoded
}

pub(super) fn parse_ego_screenshot_path(output: &str) -> Result<PathBuf, String> {
    let path = output
        .lines()
        .find_map(|line| line.strip_prefix(EGO_SCREENSHOT_PREFIX))
        .map(PathBuf::from)
        .ok_or_else(|| "ego-browser 未返回可识别的页面截图路径".to_string())?;
    let is_png = path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"));
    if !path.is_absolute() || !is_png {
        return Err(format!(
            "ego-browser 页面截图必须是绝对 PNG 路径：{}",
            path.display()
        ));
    }
    Ok(path)
}

fn is_owned_ego_temp_screenshot(path: &Path) -> bool {
    path.starts_with(std::env::temp_dir())
        && path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|name| name.starts_with("ego-browser-shot-") && name.ends_with(".png"))
}

fn decode_ego_screenshot(path: &Path) -> Result<RgbaImage, String> {
    ImageReader::open(path)
        .map_err(|error| format!("无法打开 Ego 页面截图：{error}"))?
        .with_guessed_format()
        .map_err(|error| format!("无法识别 Ego 页面截图格式：{error}"))?
        .decode()
        .map_err(|error| format!("无法解码 Ego 页面截图：{error}"))
        .map(image::DynamicImage::into_rgba8)
}
