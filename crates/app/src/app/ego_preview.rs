#[cfg(not(feature = "e2e-test-support"))]
use super::ego_page_capture::capture_ego_task_space_rgba;
#[cfg(test)]
use super::ego_page_capture::parse_ego_screenshot_path;
#[cfg(not(feature = "e2e-test-support"))]
use super::AppScreen;
use super::MultisoulApp;
use canvas::NodeId;
#[cfg(any(test, not(feature = "e2e-test-support")))]
use canvas::{BrowserSurfaceKind, CanvasNodeKind, EgoCardPhase};
use gpui::{Context, RenderImage};
#[cfg(not(feature = "e2e-test-support"))]
use image::ImageReader;
use image::{DynamicImage, Frame, RgbaImage};
#[cfg(not(feature = "e2e-test-support"))]
use screencapturekit::sc_shareable_content::SCShareableContent;
use std::collections::HashMap;
#[cfg(not(feature = "e2e-test-support"))]
use std::path::{Path, PathBuf};
#[cfg(not(feature = "e2e-test-support"))]
use std::process::Command;
#[cfg(not(feature = "e2e-test-support"))]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
#[cfg(any(test, not(feature = "e2e-test-support")))]
use std::time::Duration;
use std::time::Instant;

#[cfg(any(test, not(feature = "e2e-test-support")))]
const EGO_BUNDLE_ID: &str = "com.citrolabs.ego.lite";
#[cfg(any(test, not(feature = "e2e-test-support")))]
const EGO_APPLICATION_NAME: &str = "ego lite";
const EGO_PREVIEW_MAX_EDGE_PX: u32 = 1440;
#[cfg(any(test, not(feature = "e2e-test-support")))]
const EGO_PREVIEW_INTERVAL: Duration = Duration::from_secs(1);
#[cfg(not(feature = "e2e-test-support"))]
static EGO_PREVIEW_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
pub(crate) struct EgoPreviewStore {
    frames: HashMap<NodeId, Arc<RenderImage>>,
    errors: HashMap<NodeId, String>,
    inflight: HashMap<NodeId, u64>,
    last_requested_at: HashMap<NodeId, Instant>,
    generation: u64,
}

impl EgoPreviewStore {
    pub(crate) fn frame_for(&self, node_id: NodeId) -> Option<Arc<RenderImage>> {
        self.frames.get(&node_id).cloned()
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn replace_frame(
        &mut self,
        node_id: NodeId,
        frame: Arc<RenderImage>,
    ) -> Option<Arc<RenderImage>> {
        self.errors.remove(&node_id);
        self.frames.insert(node_id, frame)
    }

    pub(crate) fn error_for(&self, node_id: NodeId) -> Option<String> {
        self.errors.get(&node_id).cloned()
    }

    #[cfg(any(test, not(feature = "e2e-test-support")))]
    fn begin_request(&mut self, node_id: NodeId, now: Instant) -> Option<u64> {
        if self.inflight.contains_key(&node_id)
            || self
                .last_requested_at
                .get(&node_id)
                .is_some_and(|last| now.saturating_duration_since(*last) < EGO_PREVIEW_INTERVAL)
        {
            return None;
        }
        self.generation = self.generation.wrapping_add(1);
        self.last_requested_at.insert(node_id, now);
        self.inflight.insert(node_id, self.generation);
        Some(self.generation)
    }

    #[cfg(not(feature = "e2e-test-support"))]
    fn complete_request(
        &mut self,
        node_id: NodeId,
        generation: u64,
        frame: Arc<RenderImage>,
    ) -> Vec<Arc<RenderImage>> {
        if self.inflight.remove(&node_id) != Some(generation) {
            return vec![frame];
        }
        self.errors.remove(&node_id);
        self.frames.insert(node_id, frame).into_iter().collect()
    }

    #[cfg(test)]
    fn fail_request(&mut self, node_id: NodeId, generation: u64) {
        if self.inflight.get(&node_id) == Some(&generation) {
            self.inflight.remove(&node_id);
        }
    }

    #[cfg(not(feature = "e2e-test-support"))]
    fn record_error(&mut self, node_id: NodeId, generation: u64, message: String) -> bool {
        if self.inflight.remove(&node_id) != Some(generation) {
            return false;
        }
        if self.errors.get(&node_id) == Some(&message) {
            return false;
        }
        self.errors.insert(node_id, message);
        true
    }

    pub(crate) fn clear(&mut self) -> Vec<Arc<RenderImage>> {
        self.generation = self.generation.wrapping_add(1);
        self.errors.clear();
        self.inflight.clear();
        self.last_requested_at.clear();
        self.frames.drain().map(|(_, frame)| frame).collect()
    }
}

pub(super) fn render_ego_preview_from_rgba(rgba: RgbaImage) -> Arc<RenderImage> {
    let mut buffer =
        if rgba.width() > EGO_PREVIEW_MAX_EDGE_PX || rgba.height() > EGO_PREVIEW_MAX_EDGE_PX {
            DynamicImage::ImageRgba8(rgba)
                .thumbnail(EGO_PREVIEW_MAX_EDGE_PX, EGO_PREVIEW_MAX_EDGE_PX)
                .into_rgba8()
        } else {
            rgba
        };
    for pixel in buffer.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Arc::new(RenderImage::new(vec![Frame::new(buffer)]))
}

impl MultisoulApp {
    #[cfg(not(feature = "e2e-test-support"))]
    pub(super) fn refresh_due_ego_previews(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.screen, AppScreen::Canvas) || !self.window_active {
            return;
        }
        let preview_targets = eligible_ego_preview_targets(&self.runtime);
        let now = Instant::now();
        for (node_id, task_space_id, task_space_name) in preview_targets {
            let Some(generation) = self.ego_previews.begin_request(node_id, now) else {
                continue;
            };

            let work = cx.background_executor().spawn(async move {
                capture_ego_preview_frame(
                    node_id,
                    generation,
                    task_space_id.as_deref(),
                    &task_space_name,
                )
            });
            cx.spawn(async move |this, cx| {
                let result = work.await;
                let _ = this.update(cx, |app, cx| match result {
                    Ok(frame) => {
                        let released = app
                            .ego_previews
                            .complete_request(node_id, generation, frame);
                        release_frames(released, cx);
                        cx.notify();
                    }
                    Err(message) => {
                        if app
                            .ego_previews
                            .record_error(node_id, generation, message.clone())
                        {
                            tracing::warn!(node_id, %message, "Ego preview capture failed");
                            cx.notify();
                        }
                    }
                });
            })
            .detach();
        }
    }

    pub(crate) fn clear_ego_previews(&mut self, cx: &mut Context<Self>) {
        let released = self.ego_previews.clear();
        release_frames(released, cx);
    }
}

#[cfg(any(test, not(feature = "e2e-test-support")))]
fn ego_phase_allows_preview(phase: EgoCardPhase) -> bool {
    phase != EgoCardPhase::Creating
}

#[cfg(any(test, not(feature = "e2e-test-support")))]
fn eligible_ego_preview_targets(
    runtime: &runtime::Runtime,
) -> Vec<(NodeId, Option<String>, String)> {
    runtime
        .document
        .nodes()
        .iter()
        .filter_map(|node| {
            let CanvasNodeKind::Browser(spec) = &node.kind else {
                return None;
            };
            let BrowserSurfaceKind::EgoLite(ego) = &spec.surface else {
                return None;
            };
            let phase = runtime.ego_task_state(node.id)?.phase;
            if !ego_phase_allows_preview(phase) {
                return None;
            }
            Some((
                node.id,
                ego.task_space_id.clone(),
                ego.task_space_name.clone()?,
            ))
        })
        .collect()
}

#[cfg(not(feature = "e2e-test-support"))]
fn capture_ego_preview_frame(
    node_id: NodeId,
    generation: u64,
    task_space_id: Option<&str>,
    task_space_name: &str,
) -> Result<Arc<RenderImage>, String> {
    let Some(task_space_id) = task_space_id else {
        return capture_ego_window_preview(node_id, generation, task_space_name);
    };
    match capture_ego_task_space_rgba(task_space_id) {
        Ok(rgba) => Ok(render_ego_preview_from_rgba(rgba)),
        Err(page_error) => capture_ego_window_preview(node_id, generation, task_space_name)
            .map_err(|window_error| {
                format!("Ego 页面捕获失败：{page_error}；macOS 窗口捕获也失败：{window_error}")
            }),
    }
}

#[cfg(not(feature = "e2e-test-support"))]
fn capture_ego_window_preview(
    node_id: NodeId,
    generation: u64,
    task_space_name: &str,
) -> Result<Arc<RenderImage>, String> {
    let candidates = list_ego_window_candidates()?;
    if let Ok(window_id) = select_ego_window_id(&candidates, task_space_name) {
        let rgba = capture_window_rgba(window_id, node_id, generation)?;
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

#[cfg(not(feature = "e2e-test-support"))]
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

#[cfg(not(feature = "e2e-test-support"))]
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

#[cfg(not(feature = "e2e-test-support"))]
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

#[cfg(not(feature = "e2e-test-support"))]
fn decode_captured_rgba(path: &Path) -> Result<RgbaImage, String> {
    let rgba = ImageReader::open(path)
        .map_err(|error| format!("无法打开 Ego 预览图：{error}"))?
        .with_guessed_format()
        .map_err(|error| format!("无法识别 Ego 预览格式：{error}"))?
        .decode()
        .map_err(|error| format!("无法解码 Ego 预览图：{error}"))?
        .into_rgba8();
    Ok(rgba)
}

#[cfg(not(feature = "e2e-test-support"))]
fn ego_preview_temp_path(node_id: NodeId, generation: u64) -> PathBuf {
    let sequence = EGO_PREVIEW_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "norma-ego-preview-{}-{node_id}-{generation}-{sequence}.png",
        std::process::id()
    ))
}

fn release_frames(frames: Vec<Arc<RenderImage>>, cx: &mut Context<MultisoulApp>) {
    if frames.is_empty() {
        return;
    }
    cx.defer(move |cx| {
        for frame in frames {
            cx.drop_image(frame, None);
        }
    });
}

#[cfg(any(test, not(feature = "e2e-test-support")))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct EgoWindowCandidate {
    window_id: u32,
    owner_bundle_id: Option<String>,
    owner_name: Option<String>,
    title: Option<String>,
    is_active: bool,
    is_on_screen: bool,
}

#[cfg(any(test, not(feature = "e2e-test-support")))]
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

#[cfg(any(test, not(feature = "e2e-test-support")))]
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

#[cfg(any(test, not(feature = "e2e-test-support")))]
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

#[cfg(any(test, not(feature = "e2e-test-support")))]
fn select_unique_non_blank_capture(scores: &[(u32, usize)]) -> Result<u32, String> {
    const NON_BLANK_COLOR_THRESHOLD: usize = 8;
    let mut non_blank = scores
        .iter()
        .filter(|(_, score)| *score >= NON_BLANK_COLOR_THRESHOLD)
        .map(|(window_id, _)| *window_id);
    let Some(selected) = non_blank.next() else {
        return Err("同名 Ego Lite 窗口均未返回可识别的页面内容".to_string());
    };
    if non_blank.next().is_some() {
        return Err("多个同名 Ego Lite 窗口都有页面内容，无法安全选择".to_string());
    }
    Ok(selected)
}

#[cfg(not(feature = "e2e-test-support"))]
fn preview_color_score(rgba: &RgbaImage) -> usize {
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
        .collect::<std::collections::HashSet<_>>()
        .len()
}

#[cfg(test)]
#[path = "../../tests/unit/ego_preview_tests.rs"]
mod tests;
