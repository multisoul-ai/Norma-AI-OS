mod recording;

#[cfg(target_os = "macos")]
mod macos;

use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gpui::{App, Global};

use recording::{DiagnosticCapture, PerformanceRecorder, PerformanceSummary};
pub(crate) use recording::{PerformanceState, ProcessCounters, RawPerformanceSample};

const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);
const TERMINAL_ACTIVITY_TTL: Duration = Duration::from_secs(2);
const WORKER_THREAD_NAME: &str = "norma-performance-monitor";

pub(crate) struct PerformanceMonitor {
    registry: Arc<WindowSignalRegistry>,
    shutdown: Sender<()>,
    worker: Option<JoinHandle<()>>,
}

impl Global for PerformanceMonitor {}

impl Drop for PerformanceMonitor {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub(crate) fn install(cx: &mut App) {
    if cx.has_global::<PerformanceMonitor>() {
        return;
    }
    let registry = Arc::new(WindowSignalRegistry::new());
    let (shutdown, shutdown_receiver) = mpsc::channel();
    let worker = spawn_worker(registry.clone(), shutdown_receiver);
    cx.set_global(PerformanceMonitor {
        registry,
        shutdown,
        worker,
    });
}

pub(crate) fn register_window(cx: &App) -> WindowPerformanceHandle {
    let Some(monitor) = cx.try_global::<PerformanceMonitor>() else {
        return WindowPerformanceHandle::unregistered();
    };
    monitor.registry.register()
}

pub(crate) struct WindowPerformanceHandle {
    signals: Option<Arc<WindowSignals>>,
    started_at: Instant,
}

impl WindowPerformanceHandle {
    fn unregistered() -> Self {
        Self {
            signals: None,
            started_at: Instant::now(),
        }
    }

    pub(crate) fn set_state(&self, state: PerformanceState) {
        if let Some(signals) = &self.signals {
            signals.state.store(state.code(), Ordering::Relaxed);
        }
    }

    pub(crate) fn note_terminal_activity(&self) {
        if let Some(signals) = &self.signals {
            let active_until = self
                .started_at
                .elapsed()
                .saturating_add(TERMINAL_ACTIVITY_TTL)
                .as_millis()
                .min(u64::MAX as u128) as u64;
            signals
                .terminal_active_until_ms
                .store(active_until, Ordering::Relaxed);
        }
    }

    pub(crate) fn record_repaint(&self) {
        if let Some(signals) = &self.signals {
            signals.repaint_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn record_render(&self) {
        if let Some(signals) = &self.signals {
            signals.render_count.fetch_add(1, Ordering::Relaxed);
        }
    }
}

struct WindowSignals {
    state: AtomicU8,
    terminal_active_until_ms: AtomicU64,
    repaint_count: AtomicU64,
    render_count: AtomicU64,
}

impl WindowSignals {
    fn new() -> Self {
        Self {
            state: AtomicU8::new(PerformanceState::Idle.code()),
            terminal_active_until_ms: AtomicU64::new(0),
            repaint_count: AtomicU64::new(0),
            render_count: AtomicU64::new(0),
        }
    }

    fn snapshot(&self, observed_at_ms: u64) -> WindowSignalSnapshot {
        let base_state = PerformanceState::from_code(self.state.load(Ordering::Relaxed));
        let terminal_is_active =
            self.terminal_active_until_ms.load(Ordering::Relaxed) >= observed_at_ms;
        let state = if terminal_is_active
            && base_state != PerformanceState::Background
            && base_state != PerformanceState::VoiceActive
        {
            PerformanceState::TerminalActive
        } else {
            base_state
        };
        WindowSignalSnapshot {
            state,
            repaint_count: self.repaint_count.load(Ordering::Relaxed),
            render_count: self.render_count.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone, Copy)]
struct WindowSignalSnapshot {
    state: PerformanceState,
    repaint_count: u64,
    render_count: u64,
}

struct WindowSignalRegistry {
    started_at: Instant,
    windows: Mutex<Vec<Weak<WindowSignals>>>,
}

impl WindowSignalRegistry {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
            windows: Mutex::new(Vec::new()),
        }
    }

    fn register(&self) -> WindowPerformanceHandle {
        let signals = Arc::new(WindowSignals::new());
        self.lock_windows().push(Arc::downgrade(&signals));
        WindowPerformanceHandle {
            signals: Some(signals),
            started_at: self.started_at,
        }
    }

    fn snapshot(&self) -> Option<WindowSignalSnapshot> {
        let observed_at_ms = self.started_at.elapsed().as_millis().min(u64::MAX as u128) as u64;
        let mut windows = self.lock_windows();
        let mut snapshots = Vec::with_capacity(windows.len());
        windows.retain(|window| {
            if let Some(window) = window.upgrade() {
                snapshots.push(window.snapshot(observed_at_ms));
                true
            } else {
                false
            }
        });
        let mut snapshots = snapshots.into_iter();
        let first = snapshots.next()?;
        Some(
            snapshots.fold(first, |aggregate, window| WindowSignalSnapshot {
                state: if window.state.priority() > aggregate.state.priority() {
                    window.state
                } else {
                    aggregate.state
                },
                repaint_count: aggregate.repaint_count.saturating_add(window.repaint_count),
                render_count: aggregate.render_count.saturating_add(window.render_count),
            }),
        )
    }

    fn lock_windows(&self) -> std::sync::MutexGuard<'_, Vec<Weak<WindowSignals>>> {
        self.windows
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

fn spawn_worker(
    registry: Arc<WindowSignalRegistry>,
    shutdown: Receiver<()>,
) -> Option<JoinHandle<()>> {
    #[cfg(target_os = "macos")]
    {
        match thread::Builder::new()
            .name(WORKER_THREAD_NAME.into())
            .spawn(move || sample_until_shutdown(registry, shutdown))
        {
            Ok(worker) => Some(worker),
            Err(error) => {
                tracing::warn!(target: "norma_perf", %error, "performance monitor failed to start");
                None
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (registry, shutdown, WORKER_THREAD_NAME);
        None
    }
}

#[cfg(target_os = "macos")]
fn sample_until_shutdown(registry: Arc<WindowSignalRegistry>, shutdown: Receiver<()>) {
    let mut previous = None;
    let mut recorder = PerformanceRecorder::default();
    let mut sampling_error_reported = false;
    loop {
        match shutdown.recv_timeout(SAMPLE_INTERVAL) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
        let Some(window) = registry.snapshot() else {
            previous = None;
            continue;
        };
        let counters = match macos::read_process_counters(
            registry.started_at.elapsed(),
            window.repaint_count,
            window.render_count,
        ) {
            Ok(counters) => counters,
            Err(error) => {
                if !sampling_error_reported {
                    tracing::warn!(target: "norma_perf", %error, "performance sample failed");
                    sampling_error_reported = true;
                }
                previous = None;
                continue;
            }
        };
        sampling_error_reported = false;
        if let Some(previous_counters) = previous {
            if let Some(sample) = RawPerformanceSample::between(
                epoch_millis(),
                window.state,
                previous_counters,
                counters,
            ) {
                let outcome = recorder.push(sample);
                if let Some(summary) = outcome.summary {
                    log_summary(&summary);
                }
                if let Some(diagnostic) = outcome.diagnostic {
                    write_diagnostic(&diagnostic);
                }
            }
        }
        previous = Some(counters);
    }
}

#[cfg(target_os = "macos")]
fn log_summary(summary: &PerformanceSummary) {
    tracing::info!(
        target: "norma_perf",
        state = %summary.state,
        sample_count = summary.sample_count,
        cpu_median = summary.cpu_median,
        cpu_p95 = summary.cpu_p95,
        memory_mb = summary.memory_mb,
        memory_peak_mb = summary.memory_peak_mb,
        wakeups_per_s = summary.wakeups_per_second,
        thread_count = summary.thread_count,
        repaints_per_s = summary.repaints_per_second,
        renders_per_s = summary.renders_per_second,
        "target=norma_perf cpu_performance_summary"
    );
}

#[cfg(target_os = "macos")]
fn write_diagnostic(diagnostic: &DiagnosticCapture) {
    let Some(home_directory) = dirs::home_dir() else {
        tracing::warn!(target: "norma_perf", "performance diagnostic home directory unavailable");
        return;
    };
    let directory = home_directory
        .join(".norma")
        .join("diagnostics")
        .join("performance");
    let file_name = format!("norma-perf-{}-{}.json", epoch_millis(), std::process::id());
    let path = directory.join(file_name);
    if let Err(error) = persist_diagnostic(&directory, &path, diagnostic) {
        tracing::warn!(target: "norma_perf", %error, path = %path.display(), "performance diagnostic write failed");
        return;
    }
    tracing::warn!(
        target: "norma_perf",
        path = %path.display(),
        state = %diagnostic.trigger_state,
        cpu_threshold = diagnostic.cpu_threshold_percent,
        sample_count = diagnostic.samples.len(),
        "target=norma_perf cpu_high_diagnostic_written"
    );
}

#[cfg(target_os = "macos")]
fn persist_diagnostic(
    directory: &Path,
    path: &Path,
    diagnostic: &DiagnosticCapture,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    std::fs::create_dir_all(directory)?;
    let mut payload = serde_json::to_vec_pretty(diagnostic)?;
    payload.push(b'\n');
    std::fs::write(path, payload)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

#[cfg(test)]
#[path = "../tests/unit/performance_monitor_tests.rs"]
mod tests;
