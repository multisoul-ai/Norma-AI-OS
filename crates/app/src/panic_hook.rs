//! 把 panic 信息与 backtrace 同步写入 `~/.norma/logs/norma-os.panic.log`。
//!
//! 发布版 panic 之后进程会 abort，非阻塞日志线程来不及落盘，stderr 也无人接收，
//! 所以这里绕开 tracing，直接追加写文件。

use std::{
    any::Any,
    backtrace::Backtrace,
    io::Write,
    panic::PanicHookInfo,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

const PANIC_LOG_FILE: &str = "norma-os.panic.log";

pub(crate) fn install() {
    let Some(log_directory) = dirs::home_dir().map(|home| home.join(".norma").join("logs")) else {
        return;
    };
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let backtrace = Backtrace::force_capture().to_string();
        let thread = std::thread::current();
        let report = format_report(
            unix_seconds(),
            thread.name().unwrap_or("<unnamed>"),
            info,
            &backtrace,
        );
        let _ = append_report(&log_directory, &report);
        previous(info);
    }));
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn format_report(
    unix_seconds: u64,
    thread: &str,
    info: &PanicHookInfo<'_>,
    backtrace: &str,
) -> String {
    let location = info
        .location()
        .map_or_else(|| "<unknown>".to_string(), ToString::to_string);
    render_report(
        unix_seconds,
        thread,
        &location,
        payload_message(info.payload()),
        backtrace,
    )
}

fn payload_message(payload: &(dyn Any + Send)) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("<non-string panic payload>")
}

fn render_report(
    unix_seconds: u64,
    thread: &str,
    location: &str,
    message: &str,
    backtrace: &str,
) -> String {
    format!(
        "=== panic unix_time={unix_seconds} thread={thread} ===\nlocation: {location}\nmessage: {message}\nbacktrace:\n{backtrace}\n\n"
    )
}

fn append_report(log_directory: &Path, report: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(log_directory)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_directory.join(PANIC_LOG_FILE))?;
    file.write_all(report.as_bytes())?;
    file.sync_all()
}

#[cfg(test)]
#[path = "../tests/unit/panic_hook_tests.rs"]
mod tests;
