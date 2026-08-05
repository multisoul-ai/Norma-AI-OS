//! 独立 L3 runner 使用的真实 Ego 持续捕获探针。

use super::screencast::EgoScreencastStore;
use canvas::NodeId;
use std::time::{Duration, Instant};

const PROBE_NODE_ID: NodeId = 7;
const PROBE_WARMUP_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_OBSERVATION_DURATION: Duration = Duration::from_millis(2_500);
const PROBE_POLL_INTERVAL: Duration = Duration::from_millis(10);
const PROBE_MINIMUM_PRESENTED_FRAMES: usize = 55;
const PROBE_MAXIMUM_SEQUENCE_STEP: u64 = 2;

#[derive(Clone, Debug)]
pub struct EgoLiveCaptureProbeReport {
    pub task_space_name: String,
    pub presented_frames: usize,
    pub effective_fps: f64,
    pub maximum_sequence_step: u64,
    pub first_sequence: u64,
    pub last_sequence: u64,
    pub width: u32,
    pub height: u32,
    pub source: &'static str,
}

/// 在已初始化 AppKit 的主线程进程中验证真实 Ego Window 流。
///
/// 动态测试页在 2.5 秒内理论产生 60 帧；至少呈现 55 帧代表有效帧率
/// ≥22 FPS。序号步长最多为 2，防止 latest-frame 邮箱掩盖持续积压。
pub fn run_ego_live_capture_probe(
    task_space_name: &str,
) -> Result<EgoLiveCaptureProbeReport, String> {
    let targets = vec![(PROBE_NODE_ID, None, task_space_name.to_string())];
    let mut store = EgoScreencastStore::default();
    let warmup_deadline = Instant::now() + PROBE_WARMUP_TIMEOUT;
    let mut errors = Vec::new();
    let mut latest_frame = None;

    while Instant::now() < warmup_deadline && latest_frame.is_none() {
        let report = store.sync_and_poll(&targets, Instant::now());
        for (node_id, _, frame) in report.frames {
            if node_id == PROBE_NODE_ID {
                latest_frame = Some(frame);
            }
        }
        errors.extend(report.errors);
        std::thread::sleep(PROBE_POLL_INTERVAL);
    }
    if latest_frame.is_none() {
        store.shutdown();
        return Err(format!(
            "CDP screencast 未在 {} 秒内交付 Ego 首帧；错误：{errors:?}",
            PROBE_WARMUP_TIMEOUT.as_secs()
        ));
    }

    let observation_deadline = Instant::now() + PROBE_OBSERVATION_DURATION;
    let mut sequences = Vec::new();
    let mut poll_count = 0usize;
    while Instant::now() < observation_deadline {
        poll_count += 1;
        let report = store.sync_and_poll(&targets, Instant::now());
        for (node_id, sequence, frame) in report.frames {
            if node_id == PROBE_NODE_ID {
                sequences.push(sequence);
                latest_frame = Some(frame);
            }
        }
        errors.extend(report.errors);
        std::thread::yield_now();
    }

    let frame = latest_frame.ok_or_else(|| "观察结束时 Ego Live Store 丢失最后一帧".to_string())?;
    let size = frame.size(0);
    let width = u32::from(size.width);
    let height = u32::from(size.height);
    store.shutdown();

    if !errors.is_empty() {
        return Err(format!("Ego 实时流报告错误：{errors:?}"));
    }
    if sequences.len() < PROBE_MINIMUM_PRESENTED_FRAMES {
        return Err(format!(
            "Ego 实时流 2.5 秒轮询 {poll_count} 次，只呈现 {} 帧，低于 55 帧 / 22 FPS 验收线；序号范围 {:?}..{:?}",
            sequences.len(),
            sequences.first(),
            sequences.last()
        ));
    }
    if !sequences.windows(2).all(|pair| pair[1] > pair[0]) {
        return Err(format!("Ego 实时流出现重复或倒退序号：{sequences:?}"));
    }
    let maximum_sequence_step = sequences
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .max()
        .unwrap_or(0);
    if maximum_sequence_step > PROBE_MAXIMUM_SEQUENCE_STEP {
        return Err(format!(
            "Ego 实时流最大序号步长为 {maximum_sequence_step}，超过允许值 2"
        ));
    }

    let first_sequence = *sequences
        .first()
        .ok_or_else(|| "Ego 实时流没有首个呈现序号".to_string())?;
    let last_sequence = *sequences
        .last()
        .ok_or_else(|| "Ego 实时流没有末尾呈现序号".to_string())?;
    Ok(EgoLiveCaptureProbeReport {
        task_space_name: task_space_name.to_string(),
        presented_frames: sequences.len(),
        effective_fps: sequences.len() as f64 / PROBE_OBSERVATION_DURATION.as_secs_f64(),
        maximum_sequence_step,
        first_sequence,
        last_sequence,
        width,
        height,
        source: "cdp-screencast-jpeg",
    })
}
