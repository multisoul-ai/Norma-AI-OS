use portable_pty::ChildKiller;

mod observation;
pub use observation::ChildProcessObserver;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ManagedProcessIdentity {
    pid: u32,
    #[cfg(target_os = "macos")]
    start_seconds: u64,
    #[cfg(target_os = "macos")]
    start_microseconds: u64,
}

#[cfg(all(test, target_os = "macos"))]
#[path = "../tests/unit/process_observation_tests.rs"]
mod observation_tests;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ManagedProcess {
    pid: Option<u32>,
    identity: Option<ManagedProcessIdentity>,
}

pub(crate) fn capture(root_pid: Option<u32>) -> ManagedProcess {
    #[cfg(target_os = "macos")]
    let identity = root_pid.and_then(macos::process_identity);

    #[cfg(not(target_os = "macos"))]
    let identity = root_pid.map(|pid| ManagedProcessIdentity { pid });

    ManagedProcess {
        pid: root_pid,
        identity,
    }
}

impl ManagedProcess {
    pub(crate) fn pid(self) -> Option<u32> {
        self.pid
    }

    pub(crate) fn observer(self) -> Option<ChildProcessObserver> {
        ChildProcessObserver::from_managed(self)
    }
}

pub(crate) fn terminate_or_signal(
    root: ManagedProcess,
    fallback: &mut (dyn ChildKiller + Send + Sync),
) {
    #[cfg(target_os = "macos")]
    {
        if let Some(identity) = root.identity {
            if macos::terminate_managed_tree(identity) {
                return;
            }
        }
        if root.pid.is_some() {
            return;
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = root;

    let _ = fallback.kill();
}

#[cfg(all(test, target_os = "macos"))]
impl ManagedProcessIdentity {
    fn with_different_start_time_for_test(mut self) -> Self {
        self.start_microseconds = self.start_microseconds.wrapping_add(1);
        self
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::ManagedProcessIdentity;
    use std::collections::HashSet;

    const INITIAL_CHILD_CAPACITY: usize = 32;
    const MAX_CHILD_CAPACITY: usize = 4096;

    pub(super) fn terminate_managed_tree(root: ManagedProcessIdentity) -> bool {
        let Ok(root_pid) = libc::pid_t::try_from(root.pid) else {
            return false;
        };
        if root_pid <= 1 || root_pid == current_pid() {
            return false;
        }
        if process_identity(root.pid) != Some(root) {
            return true;
        }

        if !stop_process(root) {
            return process_identity(root.pid) != Some(root);
        }

        let mut seen = HashSet::from([root_pid]);
        let mut pending = vec![root_pid];
        let mut descendants = Vec::new();
        while let Some(parent_pid) = pending.pop() {
            for child_pid in child_pids(parent_pid) {
                if child_pid <= 1 || child_pid == current_pid() || !seen.insert(child_pid) {
                    continue;
                }
                let Some(child) = u32::try_from(child_pid).ok().and_then(process_identity) else {
                    continue;
                };
                if stop_process(child) {
                    descendants.push(child);
                    pending.push(child_pid);
                }
            }
        }

        for process in descendants.into_iter().rev() {
            kill_process(process);
        }
        kill_process(root);
        true
    }

    pub(super) fn process_identity(pid: u32) -> Option<ManagedProcessIdentity> {
        process_info(pid).map(|(identity, _)| identity)
    }

    pub(super) fn process_info(pid: u32) -> Option<(ManagedProcessIdentity, u32)> {
        let pid = libc::pid_t::try_from(pid).ok()?;
        if pid <= 1 {
            return None;
        }
        let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
        let info_size = std::mem::size_of::<libc::proc_bsdinfo>();
        let info_size_c = libc::c_int::try_from(info_size).ok()?;
        let bytes = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                info.as_mut_ptr().cast(),
                info_size_c,
            )
        };
        if bytes != info_size_c {
            return None;
        }
        let info = unsafe { info.assume_init() };
        if info.pbi_pid != pid as u32 {
            return None;
        }
        Some((
            ManagedProcessIdentity {
                pid: info.pbi_pid,
                start_seconds: info.pbi_start_tvsec,
                start_microseconds: info.pbi_start_tvusec,
            },
            info.pbi_ppid,
        ))
    }

    pub(super) fn child_pids(parent_pid: libc::pid_t) -> Vec<libc::pid_t> {
        checked_child_pids(parent_pid).unwrap_or_default()
    }

    pub(super) fn checked_child_pids(parent_pid: libc::pid_t) -> Option<Vec<libc::pid_t>> {
        let mut capacity = INITIAL_CHILD_CAPACITY;
        loop {
            let mut pids = vec![0 as libc::pid_t; capacity];
            let byte_len = pids
                .len()
                .saturating_mul(std::mem::size_of::<libc::pid_t>());
            let Ok(byte_len) = libc::c_int::try_from(byte_len) else {
                return None;
            };
            let (count, error) = unsafe {
                *libc::__error() = 0;
                let count =
                    libc::proc_listchildpids(parent_pid, pids.as_mut_ptr().cast(), byte_len);
                (count, *libc::__error())
            };
            if count < 0 || (count == 0 && error != 0) {
                return None;
            }
            let count = count as usize;
            if count < capacity || capacity >= MAX_CHILD_CAPACITY {
                pids.truncate(count.min(capacity));
                return Some(pids);
            }
            capacity = capacity.saturating_mul(2).min(MAX_CHILD_CAPACITY);
        }
    }

    fn current_pid() -> libc::pid_t {
        unsafe { libc::getpid() }
    }

    fn stop_process(process: ManagedProcessIdentity) -> bool {
        let Ok(pid) = libc::pid_t::try_from(process.pid) else {
            return false;
        };
        if unsafe { libc::kill(pid, libc::SIGSTOP) } != 0 {
            return false;
        }
        if process_identity(process.pid) == Some(process) {
            true
        } else {
            unsafe {
                libc::kill(pid, libc::SIGCONT);
            }
            false
        }
    }

    fn kill_process(process: ManagedProcessIdentity) {
        if process_identity(process.pid) == Some(process) {
            if let Ok(pid) = libc::pid_t::try_from(process.pid) {
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
            }
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    #[derive(Debug)]
    struct CountingFallback {
        calls: Arc<AtomicUsize>,
    }

    impl ChildKiller for CountingFallback {
        fn kill(&mut self) -> std::io::Result<()> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
            Box::new(Self {
                calls: Arc::clone(&self.calls),
            })
        }
    }

    /// 已退出 PTY 的 PID 被复用时，旧身份不得终止复用该 PID 的无关进程。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   sentinel = `/bin/sleep 30`，30s 远大于测试 50ms 的观测窗口
    ///   stale identity = sentinel 的真实 PID + 人为改变的启动微秒
    ///   fallback calls 初始值 = 0
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 启动树外 sentinel 并捕获真实进程身份
    ///   2. 只改变身份中的启动时间，模拟旧 PTY PID 已被 sentinel 复用
    ///   3. 请求终止 stale identity，等待 50ms 观察信号结果
    ///   4. 清理 sentinel，避免测试留下进程
    ///
    /// 预期结果：
    ///   - 正断言：sentinel 仍存活，证明 PID 相同但身份不同时不会误杀
    ///   - 负断言：fallback kill 调用次数仍为 0，不能绕过身份保护按裸 PID 终止
    #[test]
    fn stale_process_identity_does_not_kill_reused_pid() {
        let mut sentinel = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .expect("test should spawn unrelated sentinel process");
        let process = capture(Some(sentinel.id()));
        let identity = process
            .identity
            .expect("test should capture sentinel process identity");
        let stale_identity = identity.with_different_start_time_for_test();
        let stale_process = ManagedProcess {
            identity: Some(stale_identity),
            ..process
        };
        let calls = Arc::new(AtomicUsize::new(0));
        let mut fallback = CountingFallback {
            calls: Arc::clone(&calls),
        };

        terminate_or_signal(stale_process, &mut fallback);
        std::thread::sleep(Duration::from_millis(50));
        let sentinel_survived = macos::process_identity(sentinel.id()).is_some();
        let fallback_calls = calls.load(Ordering::SeqCst);
        let _ = sentinel.kill();
        let _ = sentinel.wait();

        assert!(
            sentinel_survived,
            "PID 相同但启动时间不同的 sentinel 不得被旧 PTY 身份误杀"
        );
        assert_eq!(
            fallback_calls, 0,
            "身份不匹配时不得退回裸 PID killer，否则仍可能误杀复用 PID"
        );
    }
}
