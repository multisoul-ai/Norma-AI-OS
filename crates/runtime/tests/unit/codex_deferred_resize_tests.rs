use super::*;

/// 场景：前台resize遇到后台安装占用PTY槽位时，不能丢失最后请求的尺寸。
/// 构造：真实测试shell以stty输出PTY尺寸，初始80×24，显式持槽位锁模拟install临界区。
/// 执行：锁内请求100×30，释放锁，由后台处理排队尺寸；查询真实PTY尺寸并统一回收。
/// 预期：即使resize当时拿不到槽位锁，后台仍将原PTY调整到30行100列。
#[test]
fn resize_during_attachment_reaches_original_pty_after_lock_releases() {
    let control = Arc::new(LaunchControl::default());
    let mut backend = DeferredPty::new(control.clone(), 80, 24);
    let slot = backend.preparation();
    let (sender, receiver) = crossbeam_channel::unbounded();
    let command = PtyCommand::new("/bin/sh").args([
        "-c",
        "stty -echo; printf READY; while IFS= read -r line; do stty size; done",
    ]);
    slot.install(control.spawn_pty(command, 80, 24, sender).unwrap())
        .unwrap();
    let mut output = String::new();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !output.contains("READY") && Instant::now() < deadline {
        if let Ok(PtyEvent::Output(bytes)) = receiver.recv_timeout(Duration::from_millis(20)) {
            output.push_str(&String::from_utf8_lossy(&bytes));
        }
    }
    slot.mark_screen_visible(); // 生产中由启动线程在首个输出时标记
    let lock = slot.pty.lock().unwrap();
    backend.resize(100, 30).unwrap();
    let worker_slot = slot.clone();
    let worker = std::thread::spawn(move || worker_slot.maintain_sizes());
    drop(lock);
    let deadline = Instant::now() + Duration::from_secs(3);
    while !output.contains("30 100") && Instant::now() < deadline {
        let _ = backend.write(b"\n");
        if let Ok(PtyEvent::Output(bytes)) = receiver.recv_timeout(Duration::from_millis(20)) {
            output.push_str(&String::from_utf8_lossy(&bytes));
        }
    }
    backend.kill();
    worker.join().unwrap();
    assert!(output.contains("READY"), "测试原PTY必须实际启动");
    assert!(
        output.contains("30 100"),
        "真实PTY必须应用最后请求的尺寸，输出={output}"
    );
}
