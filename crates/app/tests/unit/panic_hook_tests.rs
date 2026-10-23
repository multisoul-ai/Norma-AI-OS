use super::*;

/// 报告必须包含线程名、位置、消息和 backtrace；纯函数测试，不触碰进程全局 panic hook。
#[test]
fn report_contains_thread_location_message_and_backtrace() {
    let report = render_report(42, "worker", "src/a.rs:1:2", "boom 7", "frame-0");
    assert!(report.contains("unix_time=42 thread=worker"), "{report}");
    assert!(report.contains("location: src/a.rs:1:2"), "{report}");
    assert!(report.contains("message: boom 7"), "{report}");
    assert!(report.contains("frame-0"), "{report}");
}

/// payload 支持 &str 与 String，其余类型回退占位文本。
#[test]
fn payload_message_handles_str_string_and_other() {
    let str_payload: Box<dyn Any + Send> = Box::new("static");
    let string_payload: Box<dyn Any + Send> = Box::new(String::from("owned"));
    let other_payload: Box<dyn Any + Send> = Box::new(7_u32);
    assert_eq!(payload_message(str_payload.as_ref()), "static");
    assert_eq!(payload_message(string_payload.as_ref()), "owned");
    assert_eq!(
        payload_message(other_payload.as_ref()),
        "<non-string panic payload>"
    );
}

/// 多次写入必须追加而不是覆盖旧崩溃记录。
#[test]
fn append_report_appends_without_overwriting() {
    let directory = std::env::temp_dir().join(format!("norma-panic-hook-{}", std::process::id()));
    append_report(&directory, "first\n").unwrap();
    append_report(&directory, "second\n").unwrap();
    let content = std::fs::read_to_string(directory.join(PANIC_LOG_FILE)).unwrap();
    assert_eq!(content, "first\nsecond\n");
    let _ = std::fs::remove_dir_all(&directory);
}
