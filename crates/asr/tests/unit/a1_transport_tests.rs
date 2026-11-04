use super::*;
use crate::providers::local::a1::test_python as a1_python;

fn worker(mode: &str) -> Transport {
    let mut command = std::process::Command::new(a1_python::executable());
    command
        .arg("-B")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/a1_fake_worker.py"
        ))
        .arg(mode);
    Transport::spawn(command, "qwen", &Guard::default(), Duration::from_secs(3))
        .expect("协议夹具应启动")
}

/// 场景：真实子进程跨两个会话复用；构造 begin/finish 帧；执行往返。
/// 预期：两个会话返回同一个 PID；Drop 后进程确实被 wait 回收。
#[test]
fn worker_roundtrip_reuses_process_and_reaps_on_drop() {
    let mut worker = worker("ok");
    let pid = worker.child.id();
    let control = worker.control();
    let first = worker
        .request(
            1,
            1,
            "begin",
            &[],
            &Guard::default(),
            Duration::from_secs(1),
        )
        .unwrap();
    let second = worker
        .request(
            2,
            1,
            "begin",
            &[],
            &Guard::default(),
            Duration::from_secs(1),
        )
        .unwrap();
    assert_eq!(first, second, "暖会话不得重启模型进程");
    assert_eq!(first, pid.to_string(), "输出应来自真实子进程");
    drop(worker);
    assert!(control.exited(), "退出后不能遗留 worker");
}

/// 场景：引擎响应错误、降级或超过长度上限；构造三种真实 worker 响应。
/// 执行 begin 往返；预期全部失败，不能作为精准终稿接受。
#[test]
fn invalid_or_degraded_responses_never_become_final_text() {
    for mode in ["stale", "fallback", "oversized"] {
        let mut worker = worker(mode);
        assert!(
            worker
                .request(
                    1,
                    1,
                    "begin",
                    &[],
                    &Guard::default(),
                    Duration::from_secs(1)
                )
                .is_err(),
            "{mode} 应被拒绝"
        );
    }
}

/// 场景：native 推理卡住；构造 30 秒无响应子进程，100ms 后取消。
/// 执行同步请求；预期取消在 2 秒内返回，Drop 回收子进程，不等推理自然完成。
#[test]
fn cancellation_interrupts_waiting_and_reaps_worker() {
    let mut worker = worker("hang");
    let control = worker.control();
    let guard = Guard::default();
    let token = guard.session.clone();
    let cancel = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        token.cancel();
    });
    let start = Instant::now();
    assert_eq!(
        worker.request(1, 1, "begin", &[], &guard, Duration::from_secs(10)),
        Err(AsrError::SessionClosed),
        "取消必须中断等待"
    );
    drop(worker);
    cancel.join().unwrap();
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "不能等卡死推理完成"
    );
    assert!(control.exited(), "被取消的 worker 必须被回收");
}

/// 场景：无响应也没有取消；构造 30 秒阻塞 worker 和 100ms 故障预算。
/// 执行请求；预期明确超时，不能伪造预览降级为精准终稿。
#[test]
fn deadline_bounds_a_hung_worker() {
    let mut worker = worker("hang");
    let start = Instant::now();
    let error = worker
        .request(
            1,
            1,
            "finish",
            &[],
            &Guard::default(),
            Duration::from_millis(100),
        )
        .unwrap_err();
    assert!(error.to_string().contains("timeout"), "应报告超时原因");
    assert!(start.elapsed() < Duration::from_secs(2), "故障预算必须有界");
}

/// 场景：worker 立即退出但孙进程持有 stdout；数据为协议 launcher 和无关 Python 进程。
/// 执行：建立真实 Transport 后 Drop，查询孙进程状态及无关进程状态。
/// 预期：3 秒内收口并回收专属 Job 的孙进程，无关进程继续运行；不得按已退出父 PID 假报清理。
#[cfg(windows)]
#[test]
fn exited_launcher_cannot_leave_descendant_or_kill_unrelated_process() {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::{
        Foundation::{ERROR_INVALID_PARAMETER, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
    };
    struct Unrelated(std::process::Child);
    impl Drop for Unrelated {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut unrelated = Unrelated(
        std::process::Command::new(a1_python::executable())
            .args(["-c", "import time; time.sleep(30)"])
            .spawn()
            .expect("无关进程 fixture"),
    );
    let temp = tempfile::tempdir().expect("隔离根");
    let pid_file = temp.path().join("child.pid");
    let mut command = std::process::Command::new(a1_python::executable());
    command
        .arg("-B")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/a1_fake_worker.py"
        ))
        .arg("descendant")
        .arg(&pid_file);
    let worker = Transport::spawn(command, "qwen", &Guard::default(), Duration::from_secs(3))
        .expect("真实 launcher 应发送 ready");
    let pid = std::fs::read_to_string(pid_file)
        .expect("真实孙进程 PID")
        .parse::<u32>()
        .expect("PID");
    let control = worker.control();
    let exit_deadline = Instant::now() + Duration::from_secs(2);
    while !control.exited() && Instant::now() < exit_deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        control.exited(),
        "清理前 launcher 必须已经退出，不能只覆盖父进程仍存活的路径"
    );
    let start = Instant::now();
    drop(worker);
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "父进程退出后也不能永久 join 管道 reader"
    );
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(ERROR_INVALID_PARAMETER as i32),
            "权限错误不能视作回收成功"
        );
    } else {
        let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
        assert_eq!(
            unsafe { WaitForSingleObject(handle.as_raw_handle(), 2000) },
            WAIT_OBJECT_0,
            "孙进程必须实际退出"
        );
    }
    assert!(
        unrelated.0.try_wait().expect("无关进程状态").is_none(),
        "专属 Job 不得误杀无关进程"
    );
}
