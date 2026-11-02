#![cfg(windows)]
use std::ffi::c_void;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::os::windows::{io::OwnedHandle, process::CommandExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use terminal::{PtyBackend, PtyCommand, PtyHandle, TerminalGrid};

const ROLE: &str = "NORMA_WINDOWS_TREE_ROLE";
const DIRECTORY: &str = "NORMA_WINDOWS_TREE_DIRECTORY";

#[link(name = "kernel32")]
unsafe extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
    fn GetProcessTimes(
        process: *mut c_void,
        birth: *mut u64,
        exit: *mut u64,
        kernel: *mut u64,
        user: *mut u64,
    ) -> i32;
    fn WaitForSingleObject(handle: *mut c_void, timeout: u32) -> u32;
    fn TerminateProcess(process: *mut c_void, code: u32) -> i32;
}

struct Process {
    handle: OwnedHandle,
    birth: u64,
    terminate_on_drop: bool,
}

impl Process {
    fn open(pid: u32) -> Self {
        Self::try_open(pid).expect("必须打开精确进程并获取创建时间")
    }

    fn try_open(pid: u32) -> std::io::Result<Self> {
        // QUERY_LIMITED_INFORMATION | SYNCHRONIZE | TERMINATE, held for this exact birth.
        let raw = unsafe { OpenProcess(0x1000 | 0x100000 | 1, 0, pid) };
        if raw.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        let (mut birth, mut exit, mut kernel, mut user) = (0u64, 0u64, 0u64, 0u64);
        if unsafe {
            GetProcessTimes(
                handle.as_raw_handle(),
                &mut birth,
                &mut exit,
                &mut kernel,
                &mut user,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self {
            handle,
            birth,
            terminate_on_drop: false,
        })
    }

    fn running(&self) -> bool {
        match unsafe { WaitForSingleObject(self.handle.as_raw_handle(), 0) } {
            0 => false,
            258 => true,
            result => panic!(
                "进程等待API失败，不能视为已退出: {result}, {}",
                std::io::Error::last_os_error()
            ),
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if !self.terminate_on_drop {
            return;
        }
        // Never reopen a PID during cleanup: the owned handle cannot target a reused process.
        unsafe {
            TerminateProcess(self.handle.as_raw_handle(), 1);
            WaitForSingleObject(self.handle.as_raw_handle(), 3000);
        }
    }
}

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        // A descendant may have started just before an assertion or identity collection failed.
        // Only arm cleanup after checking the published birth; never kill a reopened, reused PID.
        for role in ["leaf", "middle", "root"] {
            if let Some((pid, birth)) = read_identity(&self.0.join(format!("{role}.pid"))) {
                if let Ok(mut process) = Process::try_open(pid) {
                    process.terminate_on_drop = process.birth == birth;
                }
            }
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn launch_helper(role: &str, directory: &Path) -> Command {
    let mut command = Command::new(std::env::current_exe().expect("真实测试程序"));
    command
        .args(["--exact", "windows_tree_helper", "--nocapture"])
        .env(ROLE, role)
        .env(DIRECTORY, directory);
    command
}

/// 场景：真实PTY进程树包含脱离控制台的子、孙进程，不能依赖ConPTY自动挂断。
/// 构造：root派生middle，middle以CREATE_NO_WINDOW派生leaf，各发布PID+创建时间。
/// 执行：被父测试作为独立进程调用；普通测试进程没有ROLE时立即返回。
/// 预期：父测试通过真实PtyHandle清理三个精确句柄；helper不使用ignore或模拟退出。
#[test]
fn windows_tree_helper() {
    let Ok(role) = std::env::var(ROLE) else {
        return;
    };
    let directory = PathBuf::from(std::env::var_os(DIRECTORY).expect("helper目录"));
    let mut process = Process::open(std::process::id());
    process.terminate_on_drop = false;
    std::fs::write(
        directory.join(format!("{role}.pid")),
        format!("{} {}", std::process::id(), process.birth),
    )
    .expect("发布真实进程身份");
    let next = match role.as_str() {
        "root" => Some("middle"),
        "middle" => Some("leaf"),
        _ => None,
    };
    let mut child = next.map(|next| {
        launch_helper(next, &directory)
            .creation_flags(0x08000000)
            .spawn()
            .expect("派生脱离控制台的真实后代")
    });
    if role == "root" && std::env::var_os("NORMA_WINDOWS_TREE_EXIT_ROOT").is_some() {
        let deadline = Instant::now() + Duration::from_secs(10);
        while read_identity(&directory.join("leaf.pid")).is_none()
            || !directory.join("allow-root-exit").exists()
        {
            assert!(Instant::now() < deadline, "root退出前必须实际派生leaf");
            thread::sleep(Duration::from_millis(5));
        }
        std::process::exit(7);
    }
    thread::sleep(Duration::from_secs(60));
    if let Some(child) = &mut child {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn read_identity(path: &Path) -> Option<(u32, u64)> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut words = text.split_whitespace();
    Some((words.next()?.parse().ok()?, words.next()?.parse().ok()?))
}

/// 场景：PTY主动取消必须收口完整进程树，且不能终止树外sentinel。
/// 构造：真实Rust helper构建root/middle/leaf，后两者无控制台；sentinel由测试直接启动。
/// 执行：10秒内等齐PID+创建时间，按真实Grid响应PTY，再kill并等待原3秒预算。
/// 预期：三个精确出生身份均退出，树外同程序sentinel仍活着；所有失败路径也按句柄清理。
#[test]
fn pty_kill_reaps_detached_descendants_and_preserves_unrelated_process() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间")
        .as_nanos();
    let fixture = Fixture(
        std::env::temp_dir().join(format!("norma 中文 tree {}-{unique}", std::process::id())),
    );
    std::fs::create_dir_all(&fixture.0).expect("隔离中文空格目录");
    let mut sentinel = launch_helper("sentinel", &fixture.0)
        .creation_flags(0x08000000)
        .spawn()
        .expect("树外sentinel");
    let mut sentinel_handle = Process::open(sentinel.id());
    sentinel_handle.terminate_on_drop = true;
    let (output, events) = crossbeam_channel::unbounded();
    let command = PtyCommand::new(std::env::current_exe().expect("测试程序").to_string_lossy())
        .args(["--exact", "windows_tree_helper", "--nocapture"])
        .env(ROLE, "root")
        .env(DIRECTORY, fixture.0.to_string_lossy());
    let mut pty = PtyHandle::spawn_command(command, 80, 24, output).expect("真实PTY树");
    let mut root = Process::open(PtyBackend::child_pid(&pty).expect("真实root PID"));
    root.terminate_on_drop = true;
    let mut descendants = Vec::new();
    let mut grid = TerminalGrid::new(80, 24);
    let deadline = Instant::now() + Duration::from_secs(10);
    for role in ["middle", "leaf"] {
        loop {
            for event in events.try_iter() {
                if let terminal::PtyEvent::Output(bytes) = event {
                    let reply = grid.advance(&bytes);
                    if !reply.is_empty() {
                        pty.write(&reply).expect("真实DSR响应");
                    }
                }
            }
            if let Some((pid, birth)) = read_identity(&fixture.0.join(format!("{role}.pid"))) {
                let mut process = Process::open(pid);
                assert_eq!(process.birth, birth, "不能把复用PID当作原后代: {role}");
                process.terminate_on_drop = true;
                descendants.push(process);
                break;
            }
            assert!(Instant::now() < deadline, "必须在10秒内真实启动{role}");
            thread::sleep(Duration::from_millis(5));
        }
    }
    assert!(
        root.running() && descendants.iter().all(Process::running),
        "取消前父子孙必须全部真实存活"
    );
    let started = Instant::now();
    let deadline = started + Duration::from_secs(3);
    pty.kill();
    while (root.running() || descendants.iter().any(Process::running)) && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(5));
    }
    let survivors = (
        root.running(),
        descendants.iter().map(Process::running).collect::<Vec<_>>(),
    );
    let elapsed = started.elapsed();
    let sentinel_survived = sentinel_handle.running();
    drop(descendants);
    drop(root);
    drop(sentinel_handle);
    let _ = sentinel.wait();
    assert_eq!(
        survivors,
        (false, vec![false, false]),
        "kill后3秒内必须收口父子孙，不能只杀root"
    );
    assert!(
        elapsed < Duration::from_secs(3),
        "3秒预算必须包括kill调用本身: {elapsed:?}"
    );
    assert!(
        sentinel_survived,
        "树外同程序sentinel必须存活，不能按进程名误杀"
    );
}

/// 场景：root自然exit7后，脱离控制台的后代必须收口，而不是等终端卡片释放。
/// 构造：root等leaf真实发布身份再退出；父测试保留PTY和killer，防止借Drop碰巧通过。
/// 执行：读取middle/leaf的PID+创建时间，真实处理PTY输出，3秒内等待Exited和后代退出。
/// 预期：Exited保留root的7，全部后代退出；不能把TerminateJobObject的1覆盖root退出码。
#[test]
fn natural_root_exit_reaps_descendants_and_retains_the_root_exit_code() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间")
        .as_nanos();
    let fixture = Fixture(std::env::temp_dir().join(format!(
        "norma natural tree {}-{unique}",
        std::process::id()
    )));
    std::fs::create_dir_all(&fixture.0).expect("自然退出隔离目录");
    let (output, events) = crossbeam_channel::unbounded();
    let command = PtyCommand::new(std::env::current_exe().expect("测试程序").to_string_lossy())
        .args(["--exact", "windows_tree_helper", "--nocapture"])
        .env(ROLE, "root")
        .env(DIRECTORY, fixture.0.to_string_lossy())
        .env("NORMA_WINDOWS_TREE_EXIT_ROOT", "1");
    let mut pty = PtyHandle::spawn_command(command, 80, 24, output).expect("真实自然退出PTY");
    let mut descendants = Vec::new();
    let mut grid = TerminalGrid::new(80, 24);
    let mut exit_code = None;
    let ready = Instant::now() + Duration::from_secs(10);
    for role in ["middle", "leaf"] {
        loop {
            for event in events.try_iter() {
                match event {
                    terminal::PtyEvent::Output(bytes) => {
                        let reply = grid.advance(&bytes);
                        if !reply.is_empty() {
                            pty.write(&reply).expect("真实准备DSR响应");
                        }
                    }
                    terminal::PtyEvent::Exited { code } => exit_code = Some(code),
                }
            }
            if let Some((pid, birth)) = read_identity(&fixture.0.join(format!("{role}.pid"))) {
                let mut process = Process::open(pid);
                assert_eq!(process.birth, birth, "必须持有自然退出后的原后代: {role}");
                process.terminate_on_drop = true;
                descendants.push(process);
                break;
            }
            assert!(Instant::now() < ready, "必须实际启动{role}");
            thread::sleep(Duration::from_millis(5));
        }
    }
    assert!(
        exit_code.is_none() && descendants.iter().all(Process::running),
        "触发退出前原root和后代必须全部存活"
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    std::fs::write(fixture.0.join("allow-root-exit"), "go").expect("允许真实root自然退出");
    while Instant::now() < deadline {
        for event in events.try_iter() {
            match event {
                terminal::PtyEvent::Output(bytes) => {
                    let reply = grid.advance(&bytes);
                    if !reply.is_empty() {
                        pty.write(&reply).expect("真实DSR响应");
                    }
                }
                terminal::PtyEvent::Exited { code } => exit_code = Some(code),
            }
        }
        if exit_code.is_some() && descendants.iter().all(|process| !process.running()) {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    let survivors: Vec<_> = descendants.iter().map(Process::running).collect();
    drop(descendants);
    assert_eq!(exit_code, Some(Some(7)), "自然root退出码不能被清理后代覆盖");
    assert_eq!(
        survivors,
        vec![false, false],
        "PTY和killer仍持有时也必须清理自然root的后代"
    );
}
