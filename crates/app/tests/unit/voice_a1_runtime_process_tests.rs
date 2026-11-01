use super::*;

/// 场景：运行环境缓存更新必须原子替换，不能截断旧缓存指向的文件。
/// 构造：缓存位置为指向旧配置文件的符号链接，新Python路径无需执行。
/// 执行：通过缓存发布接口保存新路径，检查旧目标与新缓存。
/// 预期：旧内容逐字保留，新缓存为普通文件并可反序列化出完整路径。
#[test]
fn voice_a1_runtime_cache_replaces_pointer_without_truncating_previous_target() {
    let temp = tempfile::tempdir().expect("隔离缓存目录");
    let previous = temp.path().join("previous.json");
    let cached = temp.path().join("python-runtime.json");
    fs::write(&previous, b"previous-configuration").expect("旧配置");
    std::os::unix::fs::symlink(&previous, &cached).expect("旧缓存链接");
    let python = temp.path().join("runtime/bin/python");
    save_cache(&cached, &python).expect("原子保存缓存");
    assert_eq!(
        fs::read(&previous).expect("旧目标"),
        b"previous-configuration",
        "发布新缓存不能经旧链接截断其他配置"
    );
    assert!(
        !fs::symlink_metadata(&cached)
            .expect("新缓存")
            .file_type()
            .is_symlink(),
        "新缓存应原子替换旧链接"
    );
    assert_eq!(
        serde_json::from_slice::<PathBuf>(&fs::read(cached).expect("新内容")).expect("完整JSON"),
        python,
        "发布的缓存必须完整保存新运行环境路径"
    );
}

/// 场景：主进程已退出、后代仍持有stdout时，运行器也必须遵守超时并收口进程组。
/// 构造：shell立即退出，sleep后代持有管道4秒；执行预算1秒，调度允许额外2秒。
/// 执行：调用运行器并测量实际耗时。
/// 预期：返回失败且小于3秒，不能在join管道线程时失去超时保护并等待4秒后报成功。
#[test]
fn voice_a1_runtime_deadline_includes_descendant_pipe_lifetime() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "sleep 4 & exit 0"]);
    let start = Instant::now();
    let result = run(command, 1, &AtomicBool::new(false));
    assert!(
        result.is_err(),
        "后代管道超过预算必须报告失败，不能把主进程退出当成全部完成"
    );
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "管道收集也必须有界，不能无条件join到后代退出"
    );
}
