//! Ego Lite 持续窗口捕获与低拷贝帧邮箱。

use canvas::NodeId;
use core_foundation::base::TCFType;
use core_video::pixel_buffer::{kCVPixelFormatType_420YpCbCr8BiPlanarFullRange, CVPixelBuffer};
use core_video::pixel_buffer_io_surface::CVPixelBufferGetIOSurface;
#[cfg(not(feature = "e2e-test-support"))]
use screencapturekit::cm_sample_buffer::CMSampleBuffer;
#[cfg(not(feature = "e2e-test-support"))]
use screencapturekit::sc_content_filter::{InitParams, SCContentFilter};
#[cfg(not(feature = "e2e-test-support"))]
use screencapturekit::sc_error_handler::StreamErrorHandler;
#[cfg(not(feature = "e2e-test-support"))]
use screencapturekit::sc_output_handler::{SCStreamOutputType, StreamOutput};
#[cfg(not(feature = "e2e-test-support"))]
use screencapturekit::sc_shareable_content::SCShareableContent;
#[cfg(not(feature = "e2e-test-support"))]
use screencapturekit::sc_stream::SCStream;
#[cfg(not(feature = "e2e-test-support"))]
use screencapturekit::sc_stream_configuration::{PixelFormat, SCStreamConfiguration};
#[cfg(not(feature = "e2e-test-support"))]
use screencapturekit::sc_types::base::{CMTime, CMTIME_FLAGS_VALID};
#[cfg(not(feature = "e2e-test-support"))]
use screencapturekit::sc_types::SCFrameStatus;
#[cfg(not(feature = "e2e-test-support"))]
use screencapturekit::sc_window::SCWindow;
use std::collections::{HashMap, HashSet};
#[cfg(not(feature = "e2e-test-support"))]
use std::ffi::c_void;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

#[cfg(not(feature = "e2e-test-support"))]
use super::snapshot::{select_ego_window_id, EgoWindowCandidate};

pub(super) const EGO_CAPTURE_FPS: u32 = 24;
const EGO_CAPTURE_MAX_EDGE_PX: u32 = 1_920;
const EGO_CAPTURE_QUEUE_DEPTH: u32 = 2;
const EGO_CAPTURE_RETRY_INTERVAL: Duration = Duration::from_secs(1);
#[cfg(not(feature = "e2e-test-support"))]
const EGO_CAPTURE_WORKER_POLL_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct EgoCaptureConfig {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) frames_per_second: u32,
    pub(super) queue_depth: u32,
    pub(super) pixel_format_fourcc: [u8; 4],
}

impl EgoCaptureConfig {
    pub(super) fn for_window(width: u32, height: u32) -> Self {
        let source_width = width.max(2);
        let source_height = height.max(2);
        let longest_edge = source_width.max(source_height);
        let scale = if longest_edge > EGO_CAPTURE_MAX_EDGE_PX {
            EGO_CAPTURE_MAX_EDGE_PX as f64 / longest_edge as f64
        } else {
            1.0
        };
        let scaled_width = even_dimension((source_width as f64 * scale).round() as u32);
        let scaled_height = even_dimension((source_height as f64 * scale).round() as u32);
        Self {
            width: scaled_width,
            height: scaled_height,
            frames_per_second: EGO_CAPTURE_FPS,
            queue_depth: EGO_CAPTURE_QUEUE_DEPTH,
            pixel_format_fourcc: *b"420f",
        }
    }
}

fn even_dimension(value: u32) -> u32 {
    let bounded = value.max(2);
    bounded - bounded % 2
}

pub(super) struct LatestSequenceMailbox<T> {
    inner: Mutex<LatestSequenceState<T>>,
}

struct LatestSequenceState<T> {
    highest_sequence: Option<u64>,
    latest: Option<(u64, T)>,
}

impl<T> Default for LatestSequenceMailbox<T> {
    fn default() -> Self {
        Self {
            inner: Mutex::new(LatestSequenceState {
                highest_sequence: None,
                latest: None,
            }),
        }
    }
}

impl<T> LatestSequenceMailbox<T> {
    pub(super) fn push(&self, sequence: u64, value: T) -> bool {
        let Ok(mut inner) = self.inner.lock() else {
            return false;
        };
        if inner
            .highest_sequence
            .is_some_and(|highest| sequence <= highest)
        {
            return false;
        }
        inner.highest_sequence = Some(sequence);
        inner.latest = Some((sequence, value));
        true
    }

    pub(super) fn take_latest(&self) -> Option<(u64, T)> {
        self.inner.lock().ok()?.latest.take()
    }
}

pub(super) struct EgoSurfaceFrame {
    buffer: CVPixelBuffer,
}

impl EgoSurfaceFrame {
    fn new(buffer: CVPixelBuffer) -> Result<Self, String> {
        let pixel_format = buffer.get_pixel_format();
        if pixel_format != kCVPixelFormatType_420YpCbCr8BiPlanarFullRange {
            return Err(format!(
                "Ego 实时帧必须是 420f，实际像素格式为 {pixel_format}"
            ));
        }
        if buffer.get_plane_count() != 2 {
            return Err(format!(
                "Ego 实时帧必须是双平面 420f，实际平面数为 {}",
                buffer.get_plane_count()
            ));
        }
        if unsafe { CVPixelBufferGetIOSurface(buffer.as_concrete_TypeRef()).is_null() } {
            return Err("Ego 实时帧必须由 IOSurface 支撑".to_string());
        }
        Ok(Self { buffer })
    }

    pub(super) fn buffer(&self) -> &CVPixelBuffer {
        &self.buffer
    }
}

// SAFETY: 构造函数已确认该 CoreVideo buffer 是只读使用的 420f
// IOSurface；捕获线程只把所有权移入邮箱，GPUI 线程只交给 surface()，
// 两端都不会锁定或修改像素内存。
unsafe impl Send for EgoSurfaceFrame {}

#[derive(Default)]
struct EgoCaptureMailbox {
    frames: LatestSequenceMailbox<EgoSurfaceFrame>,
    failure: Mutex<Option<String>>,
}

impl EgoCaptureMailbox {
    fn push_frame(&self, sequence: u64, frame: EgoSurfaceFrame) {
        self.frames.push(sequence, frame);
    }

    fn fail(&self, message: String) {
        if let Ok(mut failure) = self.failure.lock() {
            *failure = Some(message);
        }
    }

    fn take_update(&self) -> EgoCaptureUpdate {
        if let Ok(mut failure) = self.failure.lock() {
            if let Some(message) = failure.take() {
                return EgoCaptureUpdate::Failed(message);
            }
        }
        self.frames
            .take_latest()
            .map(|(sequence, frame)| EgoCaptureUpdate::Frame { sequence, frame })
            .unwrap_or(EgoCaptureUpdate::None)
    }
}

enum EgoCaptureUpdate {
    None,
    Frame {
        sequence: u64,
        frame: EgoSurfaceFrame,
    },
    Failed(String),
}

struct EgoCaptureSession {
    task_space_name: String,
    mailbox: Arc<EgoCaptureMailbox>,
    cancel: Arc<AtomicBool>,
    last_presented_sequence: Option<u64>,
}

impl EgoCaptureSession {
    #[cfg(not(feature = "e2e-test-support"))]
    fn start(task_space_name: String) -> Self {
        let mailbox = Arc::new(EgoCaptureMailbox::default());
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_mailbox = mailbox.clone();
        let worker_cancel = cancel.clone();
        let worker_task_space_name = task_space_name.clone();
        if let Err(error) = std::thread::Builder::new()
            .name("norma-ego-live-capture".to_string())
            .spawn(move || {
                run_capture_worker(&worker_task_space_name, worker_mailbox, worker_cancel);
            })
        {
            mailbox.fail(format!("无法启动 Ego 实时捕获线程：{error}"));
        }
        Self {
            task_space_name,
            mailbox,
            cancel,
            last_presented_sequence: None,
        }
    }

    fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
}

impl Drop for EgoCaptureSession {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[derive(Default)]
pub(super) struct EgoLivePreviewStore {
    sessions: HashMap<NodeId, EgoCaptureSession>,
    frames: HashMap<NodeId, EgoSurfaceFrame>,
    retry_after: HashMap<NodeId, Instant>,
}

pub(super) struct EgoLivePollReport {
    pub(super) updated_nodes: Vec<NodeId>,
    pub(super) presented_sequences: Vec<(NodeId, u64)>,
    pub(super) errors: Vec<(NodeId, String)>,
}

impl EgoLivePreviewStore {
    pub(super) fn frame_for(&self, node_id: NodeId) -> Option<CVPixelBuffer> {
        self.frames
            .get(&node_id)
            .map(|frame| frame.buffer().clone())
    }

    pub(super) fn has_frame(&self, node_id: NodeId) -> bool {
        self.frames.contains_key(&node_id)
    }

    pub(super) fn remove_frame(&mut self, node_id: NodeId) {
        self.frames.remove(&node_id);
    }

    #[cfg(not(feature = "e2e-test-support"))]
    pub(super) fn sync_and_poll(
        &mut self,
        targets: &[(NodeId, Option<String>, String)],
        preview_nodes: &HashSet<NodeId>,
        now: Instant,
    ) -> EgoLivePollReport {
        let target_names: HashMap<NodeId, &str> = targets
            .iter()
            .map(|(node_id, _, task_space_name)| (*node_id, task_space_name.as_str()))
            .collect();
        self.sessions.retain(|node_id, session| {
            target_names
                .get(node_id)
                .is_some_and(|name| *name == session.task_space_name)
        });
        retain_frames_for_preview_nodes(&mut self.frames, preview_nodes);
        self.retry_after
            .retain(|node_id, _| target_names.contains_key(node_id));

        for (node_id, _, task_space_name) in targets {
            let retry_is_due = self
                .retry_after
                .get(node_id)
                .is_none_or(|retry_at| now >= *retry_at);
            if !self.sessions.contains_key(node_id) && retry_is_due {
                self.sessions
                    .insert(*node_id, EgoCaptureSession::start(task_space_name.clone()));
            }
        }

        let mut updated_nodes = Vec::new();
        let mut presented_sequences = Vec::new();
        let mut errors = Vec::new();
        let mut failed_nodes = HashSet::new();
        for (node_id, session) in &mut self.sessions {
            match session.mailbox.take_update() {
                EgoCaptureUpdate::None => {}
                EgoCaptureUpdate::Frame { sequence, frame } => {
                    if session
                        .last_presented_sequence
                        .is_none_or(|last| sequence > last)
                    {
                        session.last_presented_sequence = Some(sequence);
                        self.frames.insert(*node_id, frame);
                        updated_nodes.push(*node_id);
                        presented_sequences.push((*node_id, sequence));
                        self.retry_after.remove(node_id);
                    }
                }
                EgoCaptureUpdate::Failed(message) => {
                    errors.push((*node_id, message));
                    failed_nodes.insert(*node_id);
                }
            }
        }
        for node_id in failed_nodes {
            self.sessions.remove(&node_id);
            self.retry_after
                .insert(node_id, now + EGO_CAPTURE_RETRY_INTERVAL);
        }
        EgoLivePollReport {
            updated_nodes,
            presented_sequences,
            errors,
        }
    }

    pub(super) fn pause(&mut self) {
        self.sessions.clear();
        self.retry_after.clear();
    }

    pub(super) fn clear(&mut self) {
        self.pause();
        self.frames.clear();
    }
}

pub(super) fn retain_frames_for_preview_nodes<T>(
    frames: &mut HashMap<NodeId, T>,
    preview_nodes: &HashSet<NodeId>,
) {
    frames.retain(|node_id, _| preview_nodes.contains(node_id));
}

#[cfg(not(feature = "e2e-test-support"))]
struct EgoStreamOutput {
    mailbox: Arc<EgoCaptureMailbox>,
    sequence: Arc<AtomicU64>,
}

#[cfg(not(feature = "e2e-test-support"))]
impl StreamOutput for EgoStreamOutput {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, output_type: SCStreamOutputType) {
        if matches!(sample.frame_status, SCFrameStatus::Suspended) {
            self.mailbox
                .fail("Ego 原生窗口已暂停合成，切换到隐藏页面实时流".to_string());
            return;
        }
        if !matches!(output_type, SCStreamOutputType::Screen)
            || !matches!(sample.frame_status, SCFrameStatus::Complete)
        {
            return;
        }
        match surface_frame_from_sample(&sample) {
            Ok(frame) => {
                let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
                self.mailbox.push_frame(sequence, frame);
            }
            Err(message) => self.mailbox.fail(message),
        }
    }
}

#[cfg(not(feature = "e2e-test-support"))]
struct EgoStreamErrorHandler {
    mailbox: Arc<EgoCaptureMailbox>,
}

#[cfg(not(feature = "e2e-test-support"))]
impl StreamErrorHandler for EgoStreamErrorHandler {
    fn on_error(&self) {
        self.mailbox
            .fail("ScreenCaptureKit 报告 Ego 窗口流错误".to_string());
    }
}

#[cfg(not(feature = "e2e-test-support"))]
fn run_capture_worker(
    task_space_name: &str,
    mailbox: Arc<EgoCaptureMailbox>,
    cancel: Arc<AtomicBool>,
) {
    let window = match select_stream_window(task_space_name) {
        Ok(window) => window,
        Err(message) => {
            mailbox.fail(message);
            return;
        }
    };
    let config = EgoCaptureConfig::for_window(window.width, window.height);
    let filter = SCContentFilter::new(InitParams::DesktopIndependentWindow(window));
    let stream_config = SCStreamConfiguration {
        width: config.width,
        height: config.height,
        queue_depth: config.queue_depth,
        minimum_frame_interval: CMTime {
            value: 1,
            timescale: config.frames_per_second as i32,
            flags: CMTIME_FLAGS_VALID,
            epoch: 0,
        },
        pixel_format: PixelFormat::YCbCr420f,
        shows_cursor: true,
        ..Default::default()
    };
    let error_handler = EgoStreamErrorHandler {
        mailbox: mailbox.clone(),
    };
    let mut stream = SCStream::new(filter, stream_config, error_handler);
    stream.add_output(
        EgoStreamOutput {
            mailbox: mailbox.clone(),
            sequence: Arc::new(AtomicU64::new(1)),
        },
        SCStreamOutputType::Screen,
    );
    if let Err(message) = stream.start_capture() {
        mailbox.fail(format!("无法启动 Ego 窗口流：{message}"));
        return;
    }
    while !cancel.load(Ordering::Acquire) {
        std::thread::sleep(EGO_CAPTURE_WORKER_POLL_INTERVAL);
    }
    if let Err(message) = stream.stop_capture() {
        tracing::debug!(%message, "stopping Ego live capture returned an error");
    }
}

#[cfg(not(feature = "e2e-test-support"))]
fn select_stream_window(task_space_name: &str) -> Result<SCWindow, String> {
    let content = SCShareableContent::try_current().map_err(|error| {
        format!("无法读取 Ego 窗口：{error}。请在系统设置中允许 Norma OS 录制屏幕。")
    })?;
    let windows: Vec<SCWindow> = content
        .windows
        .into_iter()
        .filter(|window| window.window_layer == 0 && window.width > 1 && window.height > 1)
        .collect();
    let candidates: Vec<EgoWindowCandidate> = windows
        .iter()
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
        .collect();
    let selected_id = select_ego_window_id(&candidates, task_space_name)?;
    windows
        .into_iter()
        .find(|window| window.window_id == selected_id)
        .ok_or_else(|| format!("Ego 窗口 {selected_id} 在启动实时流前已消失"))
}

#[cfg(not(feature = "e2e-test-support"))]
fn surface_frame_from_sample(sample: &CMSampleBuffer) -> Result<EgoSurfaceFrame, String> {
    let sample_ptr = std::ptr::from_ref(&*sample.sys_ref)
        .cast_mut()
        .cast::<c_void>();
    let image_buffer = unsafe { CMSampleBufferGetImageBuffer(sample_ptr) };
    if image_buffer.is_null() {
        return Err("ScreenCaptureKit 返回的 Ego 帧没有像素缓冲区".to_string());
    }
    let buffer = unsafe { CVPixelBuffer::wrap_under_get_rule(image_buffer.cast()) };
    EgoSurfaceFrame::new(buffer)
}

#[cfg(not(feature = "e2e-test-support"))]
extern "C-unwind" {
    fn CMSampleBufferGetImageBuffer(sample_buffer: *mut c_void) -> *mut c_void;
}
