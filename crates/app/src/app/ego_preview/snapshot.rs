//! Ego Lite 首帧与实时流失败时的低频截图后备路径。

use super::quality::{preview_color_score, validate_ego_preview_rgba, EGO_PREVIEW_MIN_COLOR_SCORE};
use super::render_ego_preview_from_rgba;
use crate::app::ego_page_capture::capture_ego_task_space_rgba;
use canvas::NodeId;
use gpui::RenderImage;
use image::{ImageReader, RgbaImage};
use screencapturekit::sc_shareable_content::SCShareableContent;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const EGO_BUNDLE_ID: &str = "com.citrolabs.ego.lite";
const EGO_APPLICATION_NAME: &str = "ego lite";
static EGO_PREVIEW_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn capture_ego_preview_frame(
    node_id: NodeId,
    generation: u64,
    task_space_id: Option<&str>,
    task_space_name: &str,
) -> Result<Arc<RenderImage>, String> {
    let Some(task_space_id) = task_space_id else {
        return capture_ego_window_preview(node_id, generation, task_space_name);
    };
    match capture_ego_task_space_rgba(task_space_id) {
        Ok(rgba) => match validate_ego_preview_rgba("Ego 页面捕获", &rgba) {
            Ok(_) => Ok(render_ego_preview_from_rgba(rgba)),
            Err(page_error) => capture_ego_window_preview(node_id, generation, task_space_name)
                .map_err(|window_error| {
                    format!("Ego 页面捕获失败：{page_error}；macOS 窗口捕获也失败：{window_error}")
                }),
        },
        Err(page_error) => capture_ego_window_preview(node_id, generation, task_space_name)
            .map_err(|window_error| {
                format!("Ego 页面捕获失败：{page_error}；macOS 窗口捕获也失败：{window_error}")
            }),
    }
}

fn capture_ego_window_preview(
    node_id: NodeId,
    generation: u64,
    task_space_name: &str,
) -> Result<Arc<RenderImage>, String> {
    let candidates = list_ego_window_candidates()?;
    if let Ok(window_id) = select_ego_window_id(&candidates, task_space_name) {
        let rgba = capture_window_rgba(window_id, node_id, generation)?;
        validate_ego_preview_rgba("macOS Ego 窗口捕获", &rgba)?;
        return Ok(render_ego_preview_from_rgba(rgba));
    }

    let window_ids = highest_priority_ego_window_ids(&candidates, task_space_name);
    if window_ids.is_empty() {
        return Err(format!("未找到标题为“{task_space_name}”的 Ego Lite 窗口"));
    }
    let mut captures = Vec::with_capacity(window_ids.len());
    for window_id in window_ids {
        let rgba = capture_window_rgba(window_id, node_id, generation)?;
        captures.push((window_id, preview_color_score(&rgba), rgba));
    }
    let scores: Vec<(u32, usize)> = captures
        .iter()
        .map(|(window_id, score, _)| (*window_id, *score))
        .collect();
    let selected_window_id = select_unique_non_blank_capture(&scores)?;
    let rgba = captures
        .into_iter()
        .find_map(|(window_id, _, rgba)| (window_id == selected_window_id).then_some(rgba))
        .ok_or_else(|| "Ego 预览选择结果缺少对应图像".to_string())?;
    Ok(render_ego_preview_from_rgba(rgba))
}

fn capture_window_rgba(
    window_id: u32,
    node_id: NodeId,
    generation: u64,
) -> Result<RgbaImage, String> {
    let path = ego_preview_temp_path(node_id, generation);
    let capture_result =
        capture_window_to_path(window_id, &path).and_then(|()| decode_captured_rgba(&path));
    let _ = std::fs::remove_file(&path);
    capture_result
}

fn list_ego_window_candidates() -> Result<Vec<EgoWindowCandidate>, String> {
    let content = SCShareableContent::try_current().map_err(|error| {
        format!("无法读取 Ego 窗口：{error}。请在系统设置中允许 Norma OS 录制屏幕。")
    })?;
    Ok(content
        .windows
        .into_iter()
        .filter(|window| window.window_layer == 0 && window.width > 1 && window.height > 1)
        .map(|window| {
            let owner = window.owning_application.as_ref();
            EgoWindowCandidate::new(
                window.window_id,
                owner.and_then(|application| application.bundle_identifier.as_deref()),
                owner.and_then(|application| application.application_name.as_deref()),
                window.title.as_deref(),
                window.is_active,
                window.is_on_screen,
            )
        })
        .collect())
}

fn capture_window_to_path(window_id: u32, path: &Path) -> Result<(), String> {
    let output = Command::new("/usr/sbin/screencapture")
        .arg("-x")
        .arg("-l")
        .arg(window_id.to_string())
        .arg(path)
        .output()
        .map_err(|error| format!("无法启动 macOS 窗口捕获：{error}"))?;
    if !output.status.success() {
        return Err(format!(
            "无法捕获 Ego 窗口：{}。请在系统设置中允许 Norma OS 录制屏幕。",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

fn decode_captured_rgba(path: &Path) -> Result<RgbaImage, String> {
    ImageReader::open(path)
        .map_err(|error| format!("无法打开 Ego 预览图：{error}"))?
        .with_guessed_format()
        .map_err(|error| format!("无法识别 Ego 预览格式：{error}"))?
        .decode()
        .map_err(|error| format!("无法解码 Ego 预览图：{error}"))
        .map(|image| image.into_rgba8())
}

fn ego_preview_temp_path(node_id: NodeId, generation: u64) -> PathBuf {
    let sequence = EGO_PREVIEW_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "norma-ego-preview-{}-{node_id}-{generation}-{sequence}.png",
        std::process::id()
    ))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct EgoWindowCandidate {
    window_id: u32,
    owner_bundle_id: Option<String>,
    owner_name: Option<String>,
    title: Option<String>,
    is_active: bool,
    is_on_screen: bool,
}

impl EgoWindowCandidate {
    pub(super) fn new(
        window_id: u32,
        owner_bundle_id: Option<&str>,
        owner_name: Option<&str>,
        title: Option<&str>,
        is_active: bool,
        is_on_screen: bool,
    ) -> Self {
        Self {
            window_id,
            owner_bundle_id: owner_bundle_id.map(str::to_string),
            owner_name: owner_name.map(str::to_string),
            title: title.map(str::to_string),
            is_active,
            is_on_screen,
        }
    }

    fn belongs_to_ego(&self) -> bool {
        self.owner_bundle_id.as_deref() == Some(EGO_BUNDLE_ID)
            || (self.owner_bundle_id.is_none()
                && self
                    .owner_name
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case(EGO_APPLICATION_NAME)))
    }

    fn priority(&self) -> u8 {
        u8::from(self.is_active) * 2 + u8::from(self.is_on_screen)
    }
}

pub(super) fn select_ego_window_id(
    candidates: &[EgoWindowCandidate],
    task_space_name: &str,
) -> Result<u32, String> {
    let mut matching: Vec<&EgoWindowCandidate> = candidates
        .iter()
        .filter(|candidate| {
            candidate.belongs_to_ego() && candidate.title.as_deref() == Some(task_space_name)
        })
        .collect();
    matching.sort_by_key(|candidate| std::cmp::Reverse(candidate.priority()));

    let Some(selected) = matching.first() else {
        return Err(format!("未找到标题为“{task_space_name}”的 Ego Lite 窗口"));
    };
    if matching
        .get(1)
        .is_some_and(|candidate| candidate.priority() == selected.priority())
    {
        return Err(format!(
            "找到多个同优先级的 Ego Lite 窗口，无法安全选择“{task_space_name}”"
        ));
    }
    Ok(selected.window_id)
}

fn highest_priority_ego_window_ids(
    candidates: &[EgoWindowCandidate],
    task_space_name: &str,
) -> Vec<u32> {
    let highest_priority = candidates
        .iter()
        .filter(|candidate| {
            candidate.belongs_to_ego() && candidate.title.as_deref() == Some(task_space_name)
        })
        .map(EgoWindowCandidate::priority)
        .max();
    candidates
        .iter()
        .filter(|candidate| {
            candidate.belongs_to_ego()
                && candidate.title.as_deref() == Some(task_space_name)
                && Some(candidate.priority()) == highest_priority
        })
        .map(|candidate| candidate.window_id)
        .collect()
}

pub(super) fn select_unique_non_blank_capture(scores: &[(u32, usize)]) -> Result<u32, String> {
    let mut non_blank = scores
        .iter()
        .filter(|(_, score)| *score >= EGO_PREVIEW_MIN_COLOR_SCORE)
        .map(|(window_id, _)| *window_id);
    let Some(selected) = non_blank.next() else {
        return Err("同名 Ego Lite 窗口均未返回可识别的页面内容".to_string());
    };
    if non_blank.next().is_some() {
        return Err("多个同名 Ego Lite 窗口都有页面内容，无法安全选择".to_string());
    }
    Ok(selected)
}
