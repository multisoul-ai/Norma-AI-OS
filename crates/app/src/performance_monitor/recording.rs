use std::collections::VecDeque;
use std::fmt;
use std::time::Duration;

use serde::Serialize;

const RAW_SAMPLE_CAPACITY: usize = 120;
// summary_samples = 30 min × 60 s/min ÷ 1 s/sample = 1_800 samples.
const SUMMARY_SAMPLE_COUNT: usize = 30 * 60;
const HIGH_CPU_SAMPLE_COUNT: usize = 5;
const BYTES_PER_MEBIBYTE: f64 = 1024.0 * 1024.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PerformanceState {
    Background,
    Idle,
    Animation,
    TerminalActive,
    VoiceActive,
}

impl PerformanceState {
    pub(crate) fn cpu_threshold_percent(self) -> f64 {
        match self {
            Self::Background => 2.0,
            Self::Idle => 5.0,
            Self::Animation | Self::TerminalActive | Self::VoiceActive => 20.0,
        }
    }

    pub(crate) fn priority(self) -> u8 {
        match self {
            Self::Background => 0,
            Self::Idle => 1,
            Self::Animation => 2,
            Self::TerminalActive => 3,
            Self::VoiceActive => 4,
        }
    }

    pub(crate) fn from_code(code: u8) -> Self {
        match code {
            0 => Self::Background,
            2 => Self::Animation,
            3 => Self::TerminalActive,
            4 => Self::VoiceActive,
            _ => Self::Idle,
        }
    }

    pub(crate) fn code(self) -> u8 {
        self.priority()
    }
}

impl fmt::Display for PerformanceState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Background => "background",
            Self::Idle => "idle",
            Self::Animation => "animation",
            Self::TerminalActive => "terminal_active",
            Self::VoiceActive => "voice_active",
        };
        formatter.write_str(label)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProcessCounters {
    pub(crate) observed_at: Duration,
    pub(crate) cpu_time_ns: u64,
    pub(crate) phys_footprint_bytes: u64,
    pub(crate) wakeup_count: u64,
    pub(crate) thread_count: u32,
    pub(crate) repaint_count: u64,
    pub(crate) render_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub(crate) struct RawPerformanceSample {
    pub(crate) timestamp_epoch_ms: u64,
    pub(crate) state: PerformanceState,
    pub(crate) cpu_percent: f64,
    pub(crate) phys_footprint_bytes: u64,
    pub(crate) wakeups_per_second: f64,
    pub(crate) thread_count: u32,
    pub(crate) repaints_per_second: f64,
    pub(crate) renders_per_second: f64,
}

impl RawPerformanceSample {
    pub(crate) fn between(
        timestamp_epoch_ms: u64,
        state: PerformanceState,
        previous: ProcessCounters,
        current: ProcessCounters,
    ) -> Option<Self> {
        let elapsed = current
            .observed_at
            .checked_sub(previous.observed_at)?
            .as_secs_f64();
        if elapsed <= 0.0 {
            return None;
        }
        let cpu_delta_ns = current.cpu_time_ns.saturating_sub(previous.cpu_time_ns);
        let wakeup_delta = current.wakeup_count.saturating_sub(previous.wakeup_count);
        let repaint_delta = current.repaint_count.saturating_sub(previous.repaint_count);
        let render_delta = current.render_count.saturating_sub(previous.render_count);
        Some(Self {
            timestamp_epoch_ms,
            state,
            cpu_percent: cpu_delta_ns as f64 / 1_000_000_000.0 / elapsed * 100.0,
            phys_footprint_bytes: current.phys_footprint_bytes,
            wakeups_per_second: wakeup_delta as f64 / elapsed,
            thread_count: current.thread_count,
            repaints_per_second: repaint_delta as f64 / elapsed,
            renders_per_second: render_delta as f64 / elapsed,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PerformanceSummary {
    pub(crate) state: PerformanceState,
    pub(crate) sample_count: usize,
    pub(crate) cpu_median: f64,
    pub(crate) cpu_p95: f64,
    pub(crate) memory_mb: f64,
    pub(crate) memory_peak_mb: f64,
    pub(crate) wakeups_per_second: f64,
    pub(crate) thread_count: u32,
    pub(crate) repaints_per_second: f64,
    pub(crate) renders_per_second: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct DiagnosticCapture {
    pub(crate) schema_version: u32,
    pub(crate) trigger_state: PerformanceState,
    pub(crate) cpu_threshold_percent: f64,
    pub(crate) consecutive_high_samples: usize,
    pub(crate) sample_interval_seconds: u64,
    pub(crate) samples: Vec<RawPerformanceSample>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RecorderOutcome {
    pub(crate) summary: Option<PerformanceSummary>,
    pub(crate) diagnostic: Option<DiagnosticCapture>,
}

pub(crate) struct PerformanceRecorder {
    raw_samples: VecDeque<RawPerformanceSample>,
    summary_samples: Vec<RawPerformanceSample>,
    high_cpu_state: Option<PerformanceState>,
    consecutive_high_samples: usize,
    diagnostic_emitted_for_episode: bool,
}

impl Default for PerformanceRecorder {
    fn default() -> Self {
        Self {
            raw_samples: VecDeque::with_capacity(RAW_SAMPLE_CAPACITY),
            summary_samples: Vec::with_capacity(SUMMARY_SAMPLE_COUNT),
            high_cpu_state: None,
            consecutive_high_samples: 0,
            diagnostic_emitted_for_episode: false,
        }
    }
}

impl PerformanceRecorder {
    pub(crate) fn push(&mut self, sample: RawPerformanceSample) -> RecorderOutcome {
        if self.raw_samples.len() == RAW_SAMPLE_CAPACITY {
            self.raw_samples.pop_front();
        }
        self.raw_samples.push_back(sample);
        self.summary_samples.push(sample);

        let summary = if self.summary_samples.len() == SUMMARY_SAMPLE_COUNT {
            let summary = summarize(&self.summary_samples);
            self.summary_samples.clear();
            Some(summary)
        } else {
            None
        };

        let threshold = sample.state.cpu_threshold_percent();
        let diagnostic = if sample.cpu_percent >= threshold {
            if self.high_cpu_state == Some(sample.state) {
                self.consecutive_high_samples += 1;
            } else {
                self.high_cpu_state = Some(sample.state);
                self.consecutive_high_samples = 1;
                self.diagnostic_emitted_for_episode = false;
            }
            if self.consecutive_high_samples >= HIGH_CPU_SAMPLE_COUNT
                && !self.diagnostic_emitted_for_episode
            {
                self.diagnostic_emitted_for_episode = true;
                Some(DiagnosticCapture {
                    schema_version: 1,
                    trigger_state: sample.state,
                    cpu_threshold_percent: threshold,
                    consecutive_high_samples: self.consecutive_high_samples,
                    sample_interval_seconds: 1,
                    samples: self.raw_samples.iter().copied().collect(),
                })
            } else {
                None
            }
        } else {
            self.high_cpu_state = None;
            self.consecutive_high_samples = 0;
            self.diagnostic_emitted_for_episode = false;
            None
        };

        RecorderOutcome {
            summary,
            diagnostic,
        }
    }
}

fn summarize(samples: &[RawPerformanceSample]) -> PerformanceSummary {
    let mut cpu_values = samples
        .iter()
        .map(|sample| sample.cpu_percent)
        .collect::<Vec<_>>();
    cpu_values.sort_by(f64::total_cmp);
    let p95_index = ((cpu_values.len() as f64 * 0.95).ceil() as usize)
        .saturating_sub(1)
        .min(cpu_values.len() - 1);
    let latest = samples
        .last()
        .expect("summary requires at least one sample");
    let memory_peak_bytes = samples
        .iter()
        .map(|sample| sample.phys_footprint_bytes)
        .max()
        .expect("summary requires at least one memory sample");
    PerformanceSummary {
        state: latest.state,
        sample_count: samples.len(),
        cpu_median: median(&cpu_values),
        cpu_p95: cpu_values[p95_index],
        memory_mb: latest.phys_footprint_bytes as f64 / BYTES_PER_MEBIBYTE,
        memory_peak_mb: memory_peak_bytes as f64 / BYTES_PER_MEBIBYTE,
        wakeups_per_second: mean(samples, |sample| sample.wakeups_per_second),
        thread_count: latest.thread_count,
        repaints_per_second: mean(samples, |sample| sample.repaints_per_second),
        renders_per_second: mean(samples, |sample| sample.renders_per_second),
    }
}

fn median(ordered: &[f64]) -> f64 {
    let midpoint = ordered.len() / 2;
    if ordered.len() % 2 == 0 {
        (ordered[midpoint - 1] + ordered[midpoint]) / 2.0
    } else {
        ordered[midpoint]
    }
}

fn mean(samples: &[RawPerformanceSample], value: impl Fn(&RawPerformanceSample) -> f64) -> f64 {
    samples.iter().map(value).sum::<f64>() / samples.len() as f64
}
