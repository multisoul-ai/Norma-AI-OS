//! 只读的 Agent 后代进程检查。只能让完成判定再等一个窗口，不能单独决定 Done。
//! 扫不全或系统调用失败时返回 false，调用方继续信任工作账本。

use std::collections::{HashSet, VecDeque};

const MAX_DEPTH: u8 = 8;
const MAX_VISITS: usize = 64;
const MAX_CHILDREN: usize = 64;

/// `not_before_unix_s` 起新出现、且进程名不含 `mcp` 的后代是否仍在。
pub fn has_agent_turn_descendant(root_pid: u32, not_before_unix_s: u64) -> bool {
    #[cfg(target_os = "macos")]
    {
        scan(root_pid, not_before_unix_s).unwrap_or(false)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (root_pid, not_before_unix_s);
        false
    }
}

/// 进程已退出时为 false。查不到或没有权限时当作仍在，避免误清空账本。
pub fn agent_process_alive(pid: u32) -> bool {
    #[cfg(target_os = "macos")]
    {
        let Ok(pid) = libc::pid_t::try_from(pid) else {
            return true;
        };
        if pid <= 1 {
            return true;
        }
        let alive = unsafe { libc::kill(pid, 0) } == 0;
        alive || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = pid;
        true
    }
}

#[cfg(target_os = "macos")]
fn scan(root_pid: u32, not_before_unix_s: u64) -> Option<bool> {
    let mut seen = HashSet::from([root_pid]);
    let mut queue = VecDeque::from([(root_pid, 0u8)]);
    let mut visits = 0usize;
    while let Some((pid, depth)) = queue.pop_front() {
        let children = child_pids(pid)?;
        if depth >= MAX_DEPTH {
            if !children.is_empty() {
                return None;
            }
            continue;
        }
        for child in children {
            if !seen.insert(child) {
                continue;
            }
            visits += 1;
            if visits > MAX_VISITS {
                return None;
            }
            let (name, started) = process_snapshot(child)?;
            if !name.to_ascii_lowercase().contains("mcp") && started >= not_before_unix_s {
                return Some(true);
            }
            queue.push_back((child, depth.saturating_add(1)));
        }
    }
    Some(false)
}

#[cfg(target_os = "macos")]
fn child_pids(parent: u32) -> Option<Vec<u32>> {
    let mut pids = [0 as libc::pid_t; MAX_CHILDREN];
    let byte_len = libc::c_int::try_from(std::mem::size_of_val(&pids)).ok()?;
    let parent = libc::pid_t::try_from(parent).ok()?;
    let count = unsafe { libc::proc_listchildpids(parent, pids.as_mut_ptr().cast(), byte_len) };
    if count < 0 {
        return None;
    }
    let count = usize::try_from(count).ok()?;
    if count >= MAX_CHILDREN {
        return None;
    }
    Some(
        pids[..count]
            .iter()
            .copied()
            .filter_map(|pid| u32::try_from(pid).ok())
            .filter(|pid| *pid > 1)
            .collect(),
    )
}

#[cfg(target_os = "macos")]
fn process_snapshot(pid: u32) -> Option<(String, u64)> {
    let pid_c = libc::pid_t::try_from(pid).ok()?;
    if pid_c <= 1 {
        return None;
    }
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = std::mem::size_of::<libc::proc_bsdinfo>();
    let size_c = libc::c_int::try_from(size).ok()?;
    let bytes = unsafe {
        libc::proc_pidinfo(
            pid_c,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size_c,
        )
    };
    if bytes != size_c {
        return None;
    }
    let info = unsafe { info.assume_init() };
    if info.pbi_pid != pid {
        return None;
    }
    Some((
        command_name(&info.pbi_comm),
        u64::from(info.pbi_start_tvsec),
    ))
}

#[cfg(target_os = "macos")]
fn command_name(comm: &[libc::c_char]) -> String {
    let bytes = comm
        .iter()
        .take_while(|byte| **byte != 0)
        .map(|byte| *byte as u8)
        .collect::<Vec<_>>();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::{agent_process_alive, has_agent_turn_descendant};
    use std::process::{Command, Stdio};
    use std::time::{SystemTime, UNIX_EPOCH};

    /// 本进程在判定时刻之后拉起的 sleep 必须被看成回合后代；未来时间点则没有。
    ///
    /// 数据构造：
    ///   父进程 = 当前测试进程
    ///   子进程 = `sleep 30`
    ///   not_before = 启动前 5 秒；future = 现在 + 3600 秒
    ///
    /// 执行过程：spawn → 最多重试 10 次、每次 50ms → 再拿未来时间点查询 → Drop 时杀掉 sleep。
    ///
    /// 预期结果：
    ///   - 正断言：启动后的查询为 true，且进程仍存活。
    ///   - 负断言：not_before 在未来时为 false。
    #[test]
    fn sleep_started_after_the_prompt_is_a_turn_descendant() {
        let mut child = Command::new("sleep")
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("测试应能启动 sleep");
        let pid = child.id();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("系统时间应晚于 unix 纪元")
            .as_secs();
        let not_before = now.saturating_sub(5);
        let mut found = false;
        for _ in 0..10 {
            if has_agent_turn_descendant(std::process::id(), not_before) {
                found = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let future = now.saturating_add(3_600);
        let future_found = has_agent_turn_descendant(std::process::id(), future);
        let alive = agent_process_alive(pid);
        let _ = child.kill();
        let _ = child.wait();
        assert!(found, "sleep 30 是测试进程在 5 秒窗口内的新后代");
        assert!(!future_found, "启动时间早于未来基线的进程不得算本回合后代");
        assert!(alive, "断言前 sleep 仍应存活，说明查到的不是已经退出的进程");
        assert!(
            !agent_process_alive(pid),
            "kill 之后不得再把这个 pid 当成仍在运行"
        );
    }
}
