#[cfg(test)]
use super::ego_page_capture::parse_ego_screenshot_path;
#[cfg(any(test, not(feature = "e2e-test-support")))]
mod live;
#[cfg(feature = "l3-test-support")]
mod live_probe;
#[cfg(any(test, not(feature = "e2e-test-support")))]
mod pacer;
#[cfg(any(test, not(feature = "e2e-test-support")))]
mod quality;
#[cfg(any(test, not(feature = "e2e-test-support")))]
mod screencast;
#[cfg(any(test, not(feature = "e2e-test-support")))]
mod snapshot;
#[cfg(any(test, not(feature = "e2e-test-support")))]
mod worker;
#[cfg(not(feature = "e2e-test-support"))]
use super::AppScreen;
use super::MultisoulApp;
use canvas::NodeId;
#[cfg(any(test, not(feature = "e2e-test-support")))]
use canvas::{BrowserSurfaceKind, CanvasNodeKind, EgoCardPhase};
use gpui::{Context, RenderImage};
use image::{DynamicImage, Frame, RgbaImage};
#[cfg(feature = "l3-test-support")]
pub use live_probe::{run_ego_live_capture_probe, EgoLiveCaptureProbeReport};
#[cfg(test)]
use quality::validate_ego_preview_rgba;
#[cfg(any(test, not(feature = "e2e-test-support")))]
use runtime::EgoOwnership;
#[cfg(any(test, not(feature = "e2e-test-support")))]
use snapshot::capture_ego_preview_frame;
#[cfg(test)]
use snapshot::{select_ego_window_id, select_unique_non_blank_capture, EgoWindowCandidate};
use std::collections::HashMap;
#[cfg(not(feature = "e2e-test-support"))]
use std::collections::HashSet;
use std::sync::Arc;
#[cfg(any(test, not(feature = "e2e-test-support")))]
use std::time::Duration;
use std::time::Instant;

const EGO_PREVIEW_MAX_EDGE_PX: u32 = 1440;
#[cfg(any(test, not(feature = "e2e-test-support")))]
const EGO_PREVIEW_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Default)]
pub(crate) struct EgoPreviewStore {
    frames: HashMap<NodeId, Arc<RenderImage>>,
    errors: HashMap<NodeId, String>,
    inflight: HashMap<NodeId, u64>,
    last_requested_at: HashMap<NodeId, Instant>,
    generation: u64,
    #[cfg(not(feature = "e2e-test-support"))]
    live: live::EgoLivePreviewStore,
    #[cfg(not(feature = "e2e-test-support"))]
    screencast: screencast::EgoScreencastStore,
}

impl EgoPreviewStore {
    pub(crate) fn frame_for(&self, node_id: NodeId) -> Option<Arc<RenderImage>> {
        self.frames.get(&node_id).cloned()
    }

    #[cfg(not(feature = "e2e-test-support"))]
    pub(crate) fn live_frame_for(
        &self,
        node_id: NodeId,
    ) -> Option<core_video::pixel_buffer::CVPixelBuffer> {
        self.live.frame_for(node_id)
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

    #[cfg(any(test, not(feature = "e2e-test-support")))]
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

    #[cfg(any(test, not(feature = "e2e-test-support")))]
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
        #[cfg(not(feature = "e2e-test-support"))]
        {
            self.live.clear();
            self.screencast.clear();
        }
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
        if !matches!(self.screen, AppScreen::Canvas)
            || !self.window_active
            || self.settings_panel.open
        {
            self.ego_previews.live.pause();
            self.ego_previews.screencast.pause();
            return;
        }
        let preview_targets = eligible_ego_preview_targets(&self.runtime);
        let cdp_targets = cdp_capture_targets(&self.runtime, &preview_targets);
        let now = Instant::now();
        let screencast_report = self
            .ego_previews
            .screencast
            .sync_and_poll(&cdp_targets, now);
        let mut released_frames = Vec::new();
        let mut screencast_changed = false;
        for (node_id, sequence, frame) in screencast_report.frames {
            if let Some(previous) = self.ego_previews.frames.insert(node_id, frame) {
                released_frames.push(previous);
            }
            self.ego_previews.live.remove_frame(node_id);
            self.ego_previews.errors.remove(&node_id);
            tracing::trace!(node_id, sequence, "Ego CDP screencast frame presented");
            screencast_changed = true;
        }
        for (node_id, message) in screencast_report.errors {
            if self.ego_previews.errors.get(&node_id) != Some(&message) {
                tracing::warn!(node_id, %message, "Ego CDP screencast failed");
                self.ego_previews.errors.insert(node_id, message);
                screencast_changed = true;
            }
        }
        release_frames(released_frames, cx);
        if screencast_changed {
            cx.notify();
        }

        let live_targets = secondary_capture_targets(&preview_targets, |node_id| {
            self.ego_previews.screencast.is_active(node_id)
        });
        let retained_live_nodes = preview_targets
            .iter()
            .map(|(node_id, _, _)| *node_id)
            .collect::<HashSet<_>>();
        let live_report =
            self.ego_previews
                .live
                .sync_and_poll(&live_targets, &retained_live_nodes, now);
        if let Some((node_id, sequence)) = live_report.presented_sequences.last() {
            tracing::trace!(
                node_id,
                sequence,
                presented_frames = live_report.presented_sequences.len(),
                "Ego live preview frames presented"
            );
        }
        let mut live_changed = false;
        for node_id in live_report.updated_nodes {
            self.ego_previews.errors.remove(&node_id);
            live_changed = true;
        }
        for (node_id, message) in live_report.errors {
            if self.ego_previews.errors.get(&node_id) != Some(&message) {
                tracing::warn!(node_id, %message, "Ego live preview capture failed");
                self.ego_previews.errors.insert(node_id, message);
                live_changed = true;
            }
        }
        if live_changed {
            cx.notify();
        }
        for (node_id, task_space_id, task_space_name) in preview_targets {
            if self.ego_previews.live.has_frame(node_id)
                || self.ego_previews.screencast.is_active(node_id)
            {
                continue;
            }
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

#[cfg(any(test, not(feature = "e2e-test-support")))]
fn cdp_capture_targets(
    runtime: &runtime::Runtime,
    targets: &[(NodeId, Option<String>, String)],
) -> Vec<(NodeId, Option<String>, String)> {
    targets
        .iter()
        .filter(|(node_id, _, _)| {
            runtime.ego_task_state(*node_id).is_some_and(|state| {
                state.phase == EgoCardPhase::AgentWorking
                    && matches!(state.ownership, EgoOwnership::Agent { .. })
            })
        })
        .cloned()
        .collect()
}

#[cfg(any(test, not(feature = "e2e-test-support")))]
fn secondary_capture_targets(
    targets: &[(NodeId, Option<String>, String)],
    mut primary_is_active: impl FnMut(NodeId) -> bool,
) -> Vec<(NodeId, Option<String>, String)> {
    targets
        .iter()
        .filter(|(node_id, _, _)| !primary_is_active(*node_id))
        .cloned()
        .collect()
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

#[cfg(test)]
#[path = "../../tests/unit/ego_preview_live_tests.rs"]
mod live_tests;
#[cfg(test)]
#[path = "../../tests/unit/ego_preview_quality_tests.rs"]
mod quality_tests;
#[cfg(test)]
#[path = "../../tests/unit/ego_preview_source_tests.rs"]
mod source_tests;
#[cfg(test)]
#[path = "../../tests/unit/ego_preview_tests.rs"]
mod tests;
