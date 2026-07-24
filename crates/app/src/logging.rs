use std::{
    ffi::OsStr,
    io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread::{self, JoinHandle},
    time::{Duration, SystemTime},
};
use tracing_appender::{
    non_blocking::WorkerGuard,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

const NORMA_DIRECTORY_NAME: &str = ".norma";
const LOG_DIRECTORY_NAME: &str = "logs";
const LOG_FILE_PREFIX: &str = "norma-os";
const LOG_FILE_SUFFIX: &str = "log";
const LOG_RETENTION_DAYS: u64 = 14;
const LOG_RETENTION: Duration = Duration::from_secs(LOG_RETENTION_DAYS * 24 * 60 * 60);
const CLEANUP_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const MAINTENANCE_THREAD_NAME: &str = "norma-log-maintenance";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CleanupReport {
    removed: usize,
    failed: usize,
}

pub(crate) struct LoggingGuard {
    _maintenance: Option<MaintenanceGuard>,
    _writer: WorkerGuard,
}

struct MaintenanceGuard {
    shutdown: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for MaintenanceGuard {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(crate) fn initialize() -> Option<LoggingGuard> {
    let log_directory = match default_log_directory() {
        Ok(path) => path,
        Err(error) => {
            install_console_only(format!("cannot resolve user log directory: {error}"));
            return None;
        }
    };
    if let Err(error) = std::fs::create_dir_all(&log_directory) {
        install_console_only(format!("cannot create log directory: {error}"));
        return None;
    }

    let file_appender = match build_file_appender(&log_directory) {
        Ok(appender) => appender,
        Err(error) => {
            install_console_only(format!("cannot create rolling log appender: {error}"));
            return None;
        }
    };
    let (file_writer, writer_guard) = tracing_appender::non_blocking(file_appender);

    let subscriber = tracing_subscriber::registry()
        .with(env_filter())
        .with(tracing_subscriber::fmt::layer().with_writer(io::stderr))
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(file_writer),
        );
    if let Err(error) = subscriber.try_init() {
        eprintln!("Norma OS logging initialization failed: {error}");
        return None;
    }

    let maintenance = match spawn_maintenance(log_directory.clone()) {
        Ok(guard) => Some(guard),
        Err(error) => {
            tracing::warn!(%error, "failed to start log maintenance thread");
            None
        }
    };
    tracing::info!(
        retention_days = LOG_RETENTION_DAYS,
        rotation = "daily",
        "disk logging initialized"
    );

    Some(LoggingGuard {
        _maintenance: maintenance,
        _writer: writer_guard,
    })
}

fn default_log_directory() -> io::Result<PathBuf> {
    let home_directory = dirs::home_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "user home directory is unavailable",
        )
    })?;
    Ok(log_directory(&home_directory))
}

fn log_directory(home_directory: &Path) -> PathBuf {
    home_directory
        .join(NORMA_DIRECTORY_NAME)
        .join(LOG_DIRECTORY_NAME)
}

fn build_file_appender(log_directory: &Path) -> io::Result<RollingFileAppender> {
    RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .filename_suffix(LOG_FILE_SUFFIX)
        .build(log_directory)
        .map_err(|error| io::Error::other(error.to_string()))
}

fn cleanup_expired_logs(
    log_directory: &Path,
    now: SystemTime,
    retention: Duration,
) -> io::Result<CleanupReport> {
    let mut report = CleanupReport::default();
    for entry in std::fs::read_dir(log_directory)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                report.failed += 1;
                continue;
            }
        };
        if !is_owned_log_filename(&entry.file_name()) {
            continue;
        }

        let metadata = match std::fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            Err(_) => {
                report.failed += 1;
                continue;
            }
        };
        if !metadata.file_type().is_file() {
            continue;
        }
        let modified = match metadata.modified() {
            Ok(modified) => modified,
            Err(_) => {
                report.failed += 1;
                continue;
            }
        };
        let is_expired = now
            .duration_since(modified)
            .is_ok_and(|age| age > retention);
        if !is_expired {
            continue;
        }

        match std::fs::remove_file(entry.path()) {
            Ok(()) => report.removed += 1,
            Err(_) => report.failed += 1,
        }
    }
    Ok(report)
}

fn is_owned_log_filename(file_name: &OsStr) -> bool {
    let Some(file_name) = file_name.to_str() else {
        return false;
    };
    let prefix = format!("{LOG_FILE_PREFIX}.");
    let suffix = format!(".{LOG_FILE_SUFFIX}");
    let Some(date) = file_name
        .strip_prefix(&prefix)
        .and_then(|name| name.strip_suffix(&suffix))
    else {
        return false;
    };

    date.len() == 10
        && date.bytes().enumerate().all(|(index, byte)| match index {
            4 | 7 => byte == b'-',
            _ => byte.is_ascii_digit(),
        })
}

fn install_console_only(reason: String) {
    eprintln!("Norma OS disk logging unavailable; using stderr only: {reason}");
    if let Err(error) = tracing_subscriber::fmt()
        .with_env_filter(env_filter())
        .try_init()
    {
        eprintln!("Norma OS stderr logging initialization failed: {error}");
    }
}

fn env_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())
}

fn spawn_maintenance(log_directory: PathBuf) -> io::Result<MaintenanceGuard> {
    let (shutdown, shutdown_receiver) = mpsc::channel();
    let thread = thread::Builder::new()
        .name(MAINTENANCE_THREAD_NAME.into())
        .spawn(move || {
            maintain_logs_until_shutdown(
                &log_directory,
                LOG_RETENTION,
                CLEANUP_INTERVAL,
                shutdown_receiver,
            );
        })?;
    Ok(MaintenanceGuard {
        shutdown,
        thread: Some(thread),
    })
}

fn maintain_logs_until_shutdown(
    log_directory: &Path,
    retention: Duration,
    cleanup_interval: Duration,
    shutdown: Receiver<()>,
) {
    loop {
        match cleanup_expired_logs(log_directory, SystemTime::now(), retention) {
            Ok(report) if report.failed > 0 => {
                tracing::warn!(
                    removed = report.removed,
                    failed = report.failed,
                    "historical log cleanup completed with failures"
                );
            }
            Ok(report) if report.removed > 0 => {
                tracing::info!(removed = report.removed, "historical log cleanup completed");
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, "historical log cleanup failed");
            }
        }

        match shutdown.recv_timeout(cleanup_interval) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/logging_tests.rs"]
mod logging_tests;
