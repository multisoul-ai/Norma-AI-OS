//! Tests for macOS proc_pidinfo cwd capture.

#[cfg(target_os = "macos")]
mod macos {
    use crate::macos_proc::process_cwd;

    /// MPC-1: process_cwd 对当前进程返回与 std::env::current_dir() 相同的路径。
    ///
    /// 数据构造：
    ///   pid = std::process::id()（测试进程自身）
    ///   expected = std::env::current_dir().unwrap()
    ///
    /// 执行过程：
    ///   1. 调用 process_cwd(current pid)
    ///
    /// 预期结果：
    ///   - 返回 Some(path)
    ///   - path 等于 current_dir()
    #[test]
    fn test_process_cwd_self() {
        let expected = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let pid = std::process::id();

        let got = process_cwd(pid);
        assert!(got.is_some(), "当前进程的 cwd 应能被读取，实际为 None");
        assert_eq!(
            got.unwrap(),
            expected,
            "process_cwd 返回的路径应与 current_dir() 一致"
        );
    }

    /// MPC-2: process_cwd 对不存在的 PID 返回 None，不 panic。
    ///
    /// 数据构造：
    ///   pid = u32::MAX（不可能存在的 PID）
    ///
    /// 预期结果：
    ///   - 返回 None（不 panic）
    #[test]
    fn test_process_cwd_invalid_pid_returns_none() {
        let result = process_cwd(u32::MAX);
        assert!(
            result.is_none(),
            "无效 PID 应返回 None，实际为 {:?}",
            result
        );
    }
}
