use super::*;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use windows_sys::Win32::{
    Foundation::{ERROR_INVALID_PARAMETER, WAIT_OBJECT_0},
    System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
};

/// 场景：父进程已退出但孙进程继承 stdout；数据为本测试程序的原生launcher和30秒孙进程。
/// 执行：以 1.5 秒 deadline 运行 launcher，再查询记录的孙进程 PID。
/// 预期：明确 Expired、5 秒内返回，孙进程已退出；不能永久 join 或按失效父 PID 误判清理成功。
#[test]
fn expired_parent_with_inherited_stdout_reaps_descendant_and_returns() {
    let temp = tempfile::tempdir().expect("隔离 fixture");
    let pid_file = temp.path().join("child.pid");
    let mut command = Command::new(std::env::current_exe().expect("原生fixture程序"));
    command
        .args([
            "--exact",
            "review::process::windows_tests::review_inherited_stdout_helper",
            "--nocapture",
        ])
        .env("NORMA_REVIEW_STDOUT_HELPER", "launcher")
        .env("NORMA_REVIEW_STDOUT_PID_FILE", &pid_file);
    let start = Instant::now();
    let result = run(command, start + Duration::from_millis(1500), 1024);
    assert_eq!(
        result,
        Err(ReviewError::Expired),
        "继承的 stdout 不能使已退出父进程假报完成"
    );
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "超时后不能永久等待 reader 或遗留后代"
    );
    let pid = std::fs::read_to_string(pid_file)
        .unwrap_or_else(|error| {
            panic!(
                "原生launcher必须实际启动孙进程：{error}; elapsed={:?}",
                start.elapsed()
            )
        })
        .parse::<u32>()
        .expect("真实 PID");
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(ERROR_INVALID_PARAMETER as i32),
            "仅已不存在的 PID 可以视作回收，权限错误不能算通过"
        );
    } else {
        let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
        assert_eq!(
            unsafe { WaitForSingleObject(handle.as_raw_handle(), 2000) },
            WAIT_OBJECT_0,
            "专属 Job 中的孙进程必须确实退出"
        );
    }
}

/// 场景：仅用于真实子进程中构造父退出、后代仍持有stdout的OS边界。
/// 构造：同一原生测试exe通过环境选择launcher/child，避免Python冷启动混入1.5秒契约。
/// 执行：launcher派生继承stdout/stderr的child、发布PID并正常返回；child睡眠30秒。
/// 预期：外层run必须超时收口专属Job；普通测试调用立即返回，无新增ignore。
#[test]
fn review_inherited_stdout_helper() {
    match std::env::var("NORMA_REVIEW_STDOUT_HELPER").as_deref() {
        Ok("launcher") => {
            let mut child = Command::new(std::env::current_exe().expect("原生helper"))
                .args([
                    "--exact",
                    "review::process::windows_tests::review_inherited_stdout_helper",
                    "--nocapture",
                ])
                .env("NORMA_REVIEW_STDOUT_HELPER", "child")
                .spawn()
                .expect("真实继承stdout的后代");
            std::fs::write(
                std::env::var_os("NORMA_REVIEW_STDOUT_PID_FILE").expect("PID文件"),
                child.id().to_string(),
            )
            .expect("发布实际后代PID");
            // The launcher exits normally while its background waiter and descendant still live.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Ok("child") => std::thread::sleep(Duration::from_secs(30)),
        _ => {}
    }
}
