#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use terminal::{PtyBackend, PtyCommand, PtyHandle};

const HELPER_MODE: &str = "NORMA_PTY_PROCESS_TREE_HELPER";
const PID_FILE: &str = "NORMA_PTY_PROCESS_TREE_PID_FILE";

fn process_is_alive(pid: u32) -> bool {
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

fn wait_for_file(path: &Path, timeout: Duration) -> Option<String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Ok(value) = std::fs::read_to_string(path) {
            return Some(value);
        }
        thread::sleep(Duration::from_millis(20));
    }
    None
}

fn unique_temp_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "norma-pty-process-tree-{}-{nanos}",
        std::process::id()
    ))
}

fn force_kill(pid: u32) {
    if pid > 0 {
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGKILL);
        }
    }
}

fn stop_sentinel(sentinel: &mut Child) {
    let _ = sentinel.kill();
    let _ = sentinel.wait();
}

/// PTY 进程树 helper：直接子进程忽略 SIGHUP，并派生一个独立 session 的孙进程。
///
/// 数据构造（含关键数值的推导过程）：
///   root helper = 当前 integration-test binary
///   grandchild  = 同一 binary 的 ignored helper
///   sleep       = 60s，远大于父测试最多 3s 的观测窗口，确保不会自行退出
///
/// 执行过程（逐步说明系统如何处理）：
///   1. root helper 忽略 SIGHUP，模拟 Codex launcher 不响应 clone_killer
///   2. 派生 grandchild helper → grandchild 调用 setsid() 脱离 root process group
///   3. 把 grandchild PID 写入父测试指定的文件
///   4. root 保持存活，等待被 PtyHandle::kill 清理
///
/// 预期结果：
///   - 正断言由父测试完成：root 与 grandchild 都会被清理
///   - 负断言由父测试完成：树外 sentinel 不会被清理
#[test]
#[ignore = "仅作为 pty_process_tree helper 子进程运行"]
fn pty_process_tree_root_helper() {
    if std::env::var(HELPER_MODE).as_deref() != Ok("root") {
        return;
    }
    unsafe {
        libc::signal(libc::SIGHUP, libc::SIG_IGN);
    }
    let pid_file = std::env::var(PID_FILE).expect("root helper should receive grandchild PID path");
    let current_exe = std::env::current_exe().expect("root helper should resolve test binary");
    let mut grandchild = Command::new(current_exe)
        .args([
            "--ignored",
            "--exact",
            "pty_process_tree_grandchild_helper",
            "--nocapture",
        ])
        .env(HELPER_MODE, "grandchild")
        .spawn()
        .expect("root helper should spawn grandchild helper");
    std::fs::write(pid_file, grandchild.id().to_string())
        .expect("root helper should publish grandchild PID");
    thread::spawn(move || {
        let _ = grandchild.wait();
    });
    loop {
        thread::sleep(Duration::from_secs(60));
    }
}

/// PTY 孙进程 helper：创建独立 session/process group，验证清理不只依赖根 PGID。
///
/// 数据构造（含关键数值的推导过程）：
///   sleep = 60s，远大于父测试 kill 后 500ms 的稳定窗口
///
/// 执行过程（逐步说明系统如何处理）：
///   1. grandchild 调用 setsid()，不再属于 PTY root 的 process group
///   2. 忽略 SIGHUP，避免依赖终端默认挂断行为自行退出
///   3. 持续存活，直到受管进程树清理明确终止它
///
/// 预期结果：
///   - 正断言由父测试完成：调用 PtyHandle::kill 后该 PID 消失
///   - 负断言由父测试完成：不能因为只杀根 PGID 而继续存活
#[test]
#[ignore = "仅作为 pty_process_tree helper 孙进程运行"]
fn pty_process_tree_grandchild_helper() {
    if std::env::var(HELPER_MODE).as_deref() != Ok("grandchild") {
        return;
    }
    unsafe {
        libc::setsid();
        libc::signal(libc::SIGHUP, libc::SIG_IGN);
    }
    loop {
        thread::sleep(Duration::from_secs(60));
    }
}

/// 关闭真实 PTY 时，必须终止忽略 SIGHUP 的直接子进程和独立 PGID 孙进程，且不误杀树外进程。
///
/// 数据构造（含关键数值的推导过程）：
///   PTY root      = ignored root helper，忽略 SIGHUP
///   grandchild    = ignored grandchild helper，setsid() + 忽略 SIGHUP
///   sentinel      = `/bin/sleep 30`，由父测试直接启动，不属于 PTY root 后代
///   helper ready  = 最多等待 2s = 100 次 × 20ms 文件轮询
///   kill settle   = 500ms，足够完成本机信号投递，同时远小于 helper/sentinel 的 30～60s 自然退出时间
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动树外 sentinel，并记录其 PID
///   2. 通过 PtyHandle::spawn_command 启动 root helper
///   3. root 派生独立 session 的 grandchild，并通过文件发布 PID
///   4. 调用 PtyHandle::kill，等待 500ms 后检查三个 PID
///   5. 无论断言结果如何都强制清理 helper/sentinel，避免测试自身留下进程
///
/// 预期结果：
///   - 正断言：root PID 不再存在
///   - 正断言：grandchild PID 不再存在
///   - 正断言：sentinel PID 仍然存在，证明终止范围只覆盖受管 PTY 树
///   - 负断言：不能出现 root 或 grandchild 任一残留
#[test]
fn pty_kill_terminates_managed_process_tree_without_touching_sentinel() {
    let temp_dir = unique_temp_dir();
    std::fs::create_dir_all(&temp_dir).expect("test should create isolated temp directory");
    let pid_file = temp_dir.join("grandchild.pid");
    let mut sentinel = Command::new("/bin/sleep")
        .arg("30")
        .spawn()
        .expect("test should spawn unrelated sentinel process");
    let sentinel_pid = sentinel.id();
    let current_exe = std::env::current_exe().expect("test should resolve its own binary");
    let (tx, _rx) = crossbeam_channel::unbounded();
    let command = PtyCommand::new(current_exe.to_string_lossy())
        .args([
            "--ignored",
            "--exact",
            "pty_process_tree_root_helper",
            "--nocapture",
        ])
        .env(HELPER_MODE, "root")
        .env(PID_FILE, pid_file.to_string_lossy());
    let mut pty = PtyHandle::spawn_command(command, 80, 24, tx)
        .expect("test should spawn managed PTY root helper");
    let root_pid =
        PtyBackend::child_pid(&pty).expect("spawned PTY helper should expose a root PID");
    let grandchild_pid = wait_for_file(&pid_file, Duration::from_secs(2))
        .expect("root helper should publish grandchild PID within 2 seconds")
        .trim()
        .parse::<u32>()
        .expect("published grandchild PID should be a positive u32");

    pty.kill();
    thread::sleep(Duration::from_millis(500));
    let root_survived = process_is_alive(root_pid);
    let grandchild_survived = process_is_alive(grandchild_pid);
    let sentinel_survived = process_is_alive(sentinel_pid);

    force_kill(grandchild_pid);
    force_kill(root_pid);
    stop_sentinel(&mut sentinel);
    let _ = std::fs::remove_dir_all(&temp_dir);

    assert!(
        !root_survived,
        "PTY root {root_pid} should be terminated even when it ignores SIGHUP"
    );
    assert!(
        !grandchild_survived,
        "PTY grandchild {grandchild_pid} should be terminated even after creating its own session"
    );
    assert!(
        sentinel_survived,
        "unrelated sentinel {sentinel_pid} must survive managed PTY tree shutdown"
    );
}
