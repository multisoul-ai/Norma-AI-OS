//! 隐藏 Ego Task Space 的持续 CDP screencast 后备源。
//!
//! Agent-owned Task Space 会让原生窗口进入 ScreenCaptureKit `Suspended`，
//! 但 Chromium 页面仍持续合成。这里保持单个 `ego-browser nodejs` 子进程，
//! 通过 `Page.startScreencast` 收帧；不再逐帧启动进程或读写 PNG 临时文件。

use super::live::LatestSequenceMailbox;
use super::pacer::EgoFramePacer;
use super::quality::validate_ego_preview_rgba;
use super::render_ego_preview_from_rgba;
use super::worker::EgoWorkerGuard;
use crate::app::ego::ego_browser_helper;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use canvas::NodeId;
use gpui::RenderImage;
use image::ImageReader;
use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read, Write};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

const EGO_SCREENCAST_FRAME_PREFIX: &str = "NORMA_EGO_FRAME=";
const EGO_SCREENCAST_REPEAT_PREFIX: &str = "NORMA_EGO_REPEAT=";
const EGO_SCREENCAST_FPS: u32 = 24;
const EGO_SCREENCAST_MAX_WIDTH: u32 = 1_440;
const EGO_SCREENCAST_MAX_HEIGHT: u32 = 1_000;
const EGO_SCREENCAST_JPEG_QUALITY: u32 = 75;
const EGO_SCREENCAST_FRAME_INTERVAL: Duration =
    Duration::from_nanos(1_000_000_000 / EGO_SCREENCAST_FPS as u64);
const EGO_SCREENCAST_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const EGO_SCREENCAST_WORKER_POLL_INTERVAL: Duration = Duration::from_millis(5);
const EGO_SCREENCAST_MAX_PENDING_BYTES: usize = 8 * 1_024 * 1_024;
static EGO_SCREENCAST_SOCKET_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
pub(super) struct EgoScreencastMailbox {
    frames: LatestSequenceMailbox<Arc<RenderImage>>,
    failure: Mutex<Option<String>>,
    rejected_frames: AtomicU64,
}

impl EgoScreencastMailbox {
    fn push_frame(&self, sequence: u64, frame: Arc<RenderImage>) {
        self.frames.push(sequence, frame);
    }

    fn fail(&self, message: String) {
        if let Ok(mut failure) = self.failure.lock() {
            *failure = Some(message);
        }
    }

    pub(super) fn reject_frame(&self, _message: &str) {
        self.rejected_frames.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn take_update(&self) -> EgoScreencastUpdate {
        if let Ok(mut failure) = self.failure.lock() {
            if let Some(message) = failure.take() {
                return EgoScreencastUpdate::Failed(message);
            }
        }
        self.frames
            .take_latest()
            .map(|(_, frame)| EgoScreencastUpdate::Frame { frame })
            .unwrap_or(EgoScreencastUpdate::None)
    }
}

pub(super) enum EgoScreencastUpdate {
    None,
    Frame { frame: Arc<RenderImage> },
    Failed(String),
}

struct EgoScreencastSession {
    selector: String,
    mailbox: Arc<EgoScreencastMailbox>,
    _worker: Option<EgoWorkerGuard>,
    pacer: EgoFramePacer<Arc<RenderImage>>,
}

impl EgoScreencastSession {
    fn start(selector: String) -> Self {
        let mailbox = Arc::new(EgoScreencastMailbox::default());
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_mailbox = mailbox.clone();
        let worker_cancel = cancel.clone();
        let worker_selector = selector.clone();
        let worker = match std::thread::Builder::new()
            .name("norma-ego-cdp-screencast".to_string())
            .spawn(move || {
                run_screencast_worker(&worker_selector, worker_mailbox, worker_cancel);
            }) {
            Ok(handle) => Some(EgoWorkerGuard::new(cancel, handle)),
            Err(error) => {
                mailbox.fail(format!("无法启动 Ego CDP screencast 线程：{error}"));
                None
            }
        };
        Self {
            selector,
            mailbox,
            _worker: worker,
            pacer: EgoFramePacer::new(EGO_SCREENCAST_FRAME_INTERVAL),
        }
    }

    #[cfg(feature = "l3-test-support")]
    fn shutdown(mut self) {
        if let Some(worker) = self._worker.take() {
            worker.shutdown();
        }
    }
}

#[derive(Default)]
pub(super) struct EgoScreencastStore {
    sessions: HashMap<NodeId, EgoScreencastSession>,
    retry_after: HashMap<NodeId, Instant>,
}

pub(super) struct EgoScreencastPollReport {
    pub(super) frames: Vec<(NodeId, u64, Arc<RenderImage>)>,
    pub(super) errors: Vec<(NodeId, String)>,
}

impl EgoScreencastStore {
    pub(super) fn sync_and_poll(
        &mut self,
        targets: &[(NodeId, Option<String>, String)],
        now: Instant,
    ) -> EgoScreencastPollReport {
        let selectors: HashMap<NodeId, String> = targets
            .iter()
            .map(|(node_id, task_space_id, task_space_name)| {
                (
                    *node_id,
                    task_space_id
                        .clone()
                        .unwrap_or_else(|| task_space_name.clone()),
                )
            })
            .collect();
        self.sessions.retain(|node_id, session| {
            selectors
                .get(node_id)
                .is_some_and(|selector| selector == &session.selector)
        });
        self.retry_after
            .retain(|node_id, _| selectors.contains_key(node_id));

        for (node_id, selector) in &selectors {
            let retry_is_due = self
                .retry_after
                .get(node_id)
                .is_none_or(|retry_at| now >= *retry_at);
            if !self.sessions.contains_key(node_id) && retry_is_due {
                self.sessions
                    .insert(*node_id, EgoScreencastSession::start(selector.clone()));
            }
        }

        let mut frames = Vec::new();
        let mut errors = Vec::new();
        let mut failed_nodes = HashSet::new();
        for (node_id, session) in &mut self.sessions {
            match session.mailbox.take_update() {
                EgoScreencastUpdate::None => {}
                EgoScreencastUpdate::Frame { frame, .. } => {
                    session.pacer.push(frame);
                    self.retry_after.remove(node_id);
                }
                EgoScreencastUpdate::Failed(message) => {
                    errors.push((*node_id, message));
                    failed_nodes.insert(*node_id);
                }
            }
            if !failed_nodes.contains(node_id) {
                if let Some((sequence, frame)) = session.pacer.poll(now) {
                    frames.push((*node_id, sequence, frame));
                }
            }
        }
        for node_id in failed_nodes {
            self.sessions.remove(&node_id);
            self.retry_after
                .insert(node_id, now + EGO_SCREENCAST_RETRY_INTERVAL);
        }
        EgoScreencastPollReport { frames, errors }
    }

    pub(super) fn is_active(&self, node_id: NodeId) -> bool {
        self.sessions.contains_key(&node_id)
    }

    pub(super) fn pause(&mut self) {
        self.sessions.clear();
        self.retry_after.clear();
    }

    #[cfg(feature = "l3-test-support")]
    pub(super) fn shutdown(&mut self) {
        for (_, session) in self.sessions.drain() {
            session.shutdown();
        }
        self.retry_after.clear();
    }

    pub(super) fn clear(&mut self) {
        self.pause();
    }
}

fn run_screencast_worker(
    selector: &str,
    mailbox: Arc<EgoScreencastMailbox>,
    cancel: Arc<AtomicBool>,
) {
    let socket_path = screencast_socket_path();
    let listener = match UnixListener::bind(&socket_path) {
        Ok(listener) => listener,
        Err(error) => {
            mailbox.fail(format!("无法创建 Ego screencast 本机 socket：{error}"));
            return;
        }
    };
    if let Err(error) = listener.set_nonblocking(true) {
        mailbox.fail(format!("无法设置 Ego screencast socket 为非阻塞：{error}"));
        let _ = std::fs::remove_file(&socket_path);
        return;
    }
    let script = match screencast_script(selector, &socket_path) {
        Ok(script) => script,
        Err(message) => {
            mailbox.fail(message);
            let _ = std::fs::remove_file(&socket_path);
            return;
        }
    };
    let mut child = match Command::new(ego_browser_helper())
        .arg("nodejs")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            mailbox.fail(format!("无法启动 ego-browser screencast：{error}"));
            let _ = std::fs::remove_file(&socket_path);
            return;
        }
    };
    let Some(mut stdin) = child.stdin.take() else {
        mailbox.fail("ego-browser screencast 缺少 stdin".to_string());
        stop_screencast_child(&mut child, &socket_path);
        return;
    };
    if let Err(error) = stdin.write_all(script.as_bytes()) {
        mailbox.fail(format!("无法写入 ego-browser screencast 脚本：{error}"));
        stop_screencast_child(&mut child, &socket_path);
        return;
    }
    drop(stdin);

    let mut stream = loop {
        if cancel.load(Ordering::Acquire) {
            stop_screencast_child(&mut child, &socket_path);
            return;
        }
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if let Some(status) = child.try_wait().ok().flatten() {
                    mailbox.fail(format!("ego-browser screencast 连接前已退出：{status}"));
                    let _ = std::fs::remove_file(&socket_path);
                    return;
                }
                std::thread::sleep(EGO_SCREENCAST_WORKER_POLL_INTERVAL);
            }
            Err(error) => {
                mailbox.fail(format!("无法接受 Ego screencast socket：{error}"));
                stop_screencast_child(&mut child, &socket_path);
                return;
            }
        }
    };
    if let Err(error) = stream.set_nonblocking(true) {
        mailbox.fail(format!("无法设置 Ego screencast stream 为非阻塞：{error}"));
        stop_screencast_child(&mut child, &socket_path);
        return;
    }

    let mut read_buffer = [0_u8; 64 * 1_024];
    let mut pending = Vec::new();
    let mut last_decoded_frame: Option<Arc<RenderImage>> = None;
    loop {
        if cancel.load(Ordering::Acquire) {
            stop_screencast_child(&mut child, &socket_path);
            return;
        }
        match stream.read(&mut read_buffer) {
            Ok(0) => {
                let status = child
                    .wait()
                    .map(|status| status.to_string())
                    .unwrap_or_else(|error| format!("wait failed: {error}"));
                mailbox.fail(format!("ego-browser screencast socket 已关闭：{status}"));
                let _ = std::fs::remove_file(&socket_path);
                return;
            }
            Ok(read) => {
                pending.extend_from_slice(&read_buffer[..read]);
                if pending.len() > EGO_SCREENCAST_MAX_PENDING_BYTES {
                    mailbox.fail("Ego screencast socket 积压超过 8 MiB".to_string());
                    stop_screencast_child(&mut child, &socket_path);
                    return;
                }
                while let Some(newline) = pending.iter().position(|byte| *byte == b'\n') {
                    let line = String::from_utf8_lossy(&pending[..newline]);
                    match parse_screencast_repeat_line(&line) {
                        Ok(Some(sequence)) => {
                            if let Some(frame) = &last_decoded_frame {
                                mailbox.push_frame(sequence, frame.clone());
                            }
                        }
                        Ok(None) => match decode_screencast_frame_line(&line) {
                            Ok(Some((sequence, frame))) => {
                                last_decoded_frame = Some(frame.clone());
                                mailbox.push_frame(sequence, frame);
                            }
                            Ok(None) => {}
                            Err(message) => mailbox.reject_frame(&message),
                        },
                        Err(message) => mailbox.reject_frame(&message),
                    }
                    pending.drain(..=newline);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if let Some(status) = child.try_wait().ok().flatten() {
                    mailbox.fail(format!("ego-browser screencast 已退出：{status}"));
                    let _ = std::fs::remove_file(&socket_path);
                    return;
                }
                std::thread::sleep(EGO_SCREENCAST_WORKER_POLL_INTERVAL);
            }
            Err(error) => {
                mailbox.fail(format!("读取 Ego screencast socket 失败：{error}"));
                stop_screencast_child(&mut child, &socket_path);
                return;
            }
        }
    }
}

fn screencast_socket_path() -> PathBuf {
    let sequence = EGO_SCREENCAST_SOCKET_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "norma-ego-screencast-{}-{sequence}.sock",
        std::process::id()
    ))
}

fn stop_screencast_child(child: &mut std::process::Child, socket_path: &Path) {
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(socket_path);
}

pub(super) fn screencast_script(selector: &str, socket_path: &Path) -> Result<String, String> {
    let selector = serde_json::to_string(selector)
        .map_err(|error| format!("无法编码 Ego screencast Task Space：{error}"))?;
    let socket_path = serde_json::to_string(&socket_path.to_string_lossy())
        .map_err(|error| format!("无法编码 Ego screencast socket 路径：{error}"))?;
    Ok(format!(
        r#"const taskSelector = {selector};
const net = await import("node:net");
const socket = net.createConnection({{ path: {socket_path} }});
let disconnected = false;
socket.once("close", () => {{ disconnected = true; }});
socket.once("error", () => {{ disconnected = true; }});
await new Promise((resolve, reject) => {{
  socket.once("connect", resolve);
  socket.once("error", reject);
}});
await useOrCreateTaskSpace(taskSelector);
const tab = await ensureRealTab();
if (!tab) throw new Error("Ego Task Space has no real page tab");
await drainEvents();
await cdp("Page.startScreencast", {{
  format: "jpeg",
  quality: {EGO_SCREENCAST_JPEG_QUALITY},
  maxWidth: {EGO_SCREENCAST_MAX_WIDTH},
  maxHeight: {EGO_SCREENCAST_MAX_HEIGHT},
  everyNthFrame: 1
}});
socket.write("NORMA_EGO_SCREENCAST_READY\n");
let sequence = 1;
let latestFrame = null;
let hasFreshFrame = false;
let nextEmitAt = Date.now();
const frameIntervalMs = 1000 / {EGO_SCREENCAST_FPS};
while (!disconnected) {{
  await wait(0.002);
  const events = await drainEvents();
  for (const event of events) {{
    if (event.method !== "Page.screencastFrame") continue;
    latestFrame = event.params.data;
    hasFreshFrame = true;
    await cdp("Page.screencastFrameAck", {{ sessionId: event.params.sessionId }});
  }}
  const now = Date.now();
  if (latestFrame && now >= nextEmitAt) {{
    const packet = hasFreshFrame
      ? "{EGO_SCREENCAST_FRAME_PREFIX}" + sequence + ":" + latestFrame + "\n"
      : "{EGO_SCREENCAST_REPEAT_PREFIX}" + sequence + "\n";
    if (!socket.write(packet)) {{
      await new Promise(resolve => socket.once("drain", resolve));
    }}
    sequence += 1;
    hasFreshFrame = false;
    nextEmitAt += frameIntervalMs;
    if (nextEmitAt < now) nextEmitAt = now;
  }}
}}
await cdp("Page.stopScreencast").catch(() => {{}});
socket.destroy();
"#
    ))
}

pub(super) fn parse_screencast_frame_line(line: &str) -> Result<Option<(u64, &str)>, String> {
    let Some(payload) = line.strip_prefix(EGO_SCREENCAST_FRAME_PREFIX) else {
        return Ok(None);
    };
    let Some((sequence, encoded)) = payload.split_once(':') else {
        return Err("Ego screencast 帧缺少序号分隔符".to_string());
    };
    let sequence = sequence
        .parse::<u64>()
        .map_err(|error| format!("Ego screencast 帧序号无效：{error}"))?;
    if encoded.is_empty() {
        return Err("Ego screencast 帧没有 JPEG 数据".to_string());
    }
    Ok(Some((sequence, encoded)))
}

pub(super) fn parse_screencast_repeat_line(line: &str) -> Result<Option<u64>, String> {
    let Some(sequence) = line.strip_prefix(EGO_SCREENCAST_REPEAT_PREFIX) else {
        return Ok(None);
    };
    sequence
        .parse::<u64>()
        .map(Some)
        .map_err(|error| format!("Ego screencast 重复帧序号无效：{error}"))
}

fn decode_screencast_frame_line(line: &str) -> Result<Option<(u64, Arc<RenderImage>)>, String> {
    let Some((sequence, encoded)) = parse_screencast_frame_line(line)? else {
        return Ok(None);
    };
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|error| format!("Ego screencast JPEG base64 无效：{error}"))?;
    let rgba = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| format!("无法识别 Ego screencast 图像：{error}"))?
        .decode()
        .map_err(|error| format!("无法解码 Ego screencast JPEG：{error}"))?
        .into_rgba8();
    validate_ego_preview_rgba("Ego CDP screencast", &rgba)?;
    Ok(Some((sequence, render_ego_preview_from_rgba(rgba))))
}
