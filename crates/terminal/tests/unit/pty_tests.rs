use super::*;
use crossbeam_channel::unbounded;
use std::time::Duration;

/// 冒烟集成测试：spawn `/bin/sh`，写入命令 `printf 'hello-pty'; exit`，
/// 从通道收到含该文本的输出，最终收到 Exited。
///
/// 说明：这是真实进程 + 真实 PTY 的集成测试（非纯逻辑单测），在本机 macOS 有 TTY 环境下稳定。
///
/// 数据构造：
///   命令通过 stdin 写入 sh：`printf 'hello-pty'; exit\n`
///   输出预期包含 "hello-pty"（printf 不加换行，PTY 原样输出）
///
/// 执行过程：
///   1. spawn /bin/sh → 读线程启动
///   2. write `printf 'hello-pty'; exit\n` → sh 执行命令
///   3. printf 输出 "hello-pty" → Output 事件入通道
///   4. exit → sh 退出 → 读线程读到 EOF → Exited 入通道
///
/// 预期结果：
///   - 正断言：累积输出字节中包含子串 "hello-pty"
///   - 正断言：在 3 秒内收到至少一个 Exited 事件
#[test]
fn test_spawn_echo_emits_output_and_exit() {
    let (tx, rx) = unbounded();
    let mut pty = PtyHandle::spawn("/bin/sh", 80, 24, tx).expect("spawn sh");
    pty.write(b"printf 'hello-pty'; exit\n").expect("write cmd");

    let mut acc = Vec::new();
    let mut saw_exit = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(PtyEvent::Output(b)) => acc.extend_from_slice(&b),
            Ok(PtyEvent::Exited { .. }) => {
                saw_exit = true;
                break;
            }
            Err(_) => {}
        }
    }
    let text = String::from_utf8_lossy(&acc);
    assert!(
        text.contains("hello-pty"),
        "PTY 输出应包含 'hello-pty'，实际收到: {:?}",
        text
    );
    assert!(
        !text.contains("command not found"),
        "PTY 输出不应含错误信息，若有说明命令执行失败: {:?}",
        text
    );
    assert!(saw_exit, "shell exit 后应在超时内收到 Exited 事件");
}

/// spawn_command 为子进程补充 true color 终端环境变量。
///
/// 数据构造：
///   command = /bin/sh -c "printf '%s/%s' \"$TERM\" \"$COLORTERM\""
///   终端尺寸：80 列 × 24 行
///   预期环境：TERM=xterm-256color，COLORTERM=truecolor
///
/// 执行过程：
///   1. spawn_command(command, 80, 24) → PtyHandle 创建 PTY 并启动 /bin/sh
///   2. sh 读取 TERM/COLORTERM 并输出 "xterm-256color/truecolor"
///   3. 读线程把输出写入 Output 事件，进程退出后写入 Exited
///
/// 预期结果：
///   - 正断言：输出包含 "xterm-256color/truecolor"
///   - 负断言：输出不应包含 "/\r" 或 "/" 后为空，否则 COLORTERM 未注入
///   - 正断言：在 3 秒内收到 Exited
#[test]
fn test_spawn_command_sets_truecolor_terminal_environment() {
    let (tx, rx) = unbounded();
    let command =
        PtyCommand::new("/bin/sh").args(["-c", "printf '%s/%s' \"$TERM\" \"$COLORTERM\""]);
    let _pty = PtyHandle::spawn_command(command, 80, 24, tx).expect("spawn env command");

    let mut acc = Vec::new();
    let mut saw_exit = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(PtyEvent::Output(b)) => acc.extend_from_slice(&b),
            Ok(PtyEvent::Exited { .. }) => {
                saw_exit = true;
                break;
            }
            Err(_) => {}
        }
    }

    let text = String::from_utf8_lossy(&acc);
    assert!(
        text.contains("xterm-256color/truecolor"),
        "子进程应看到 true color 终端环境，实际输出: {:?}",
        text
    );
    assert!(
        !text.contains("/\r") && !text.ends_with('/'),
        "COLORTERM 不应为空，否则 true color 程序可能不会输出 24-bit 颜色: {:?}",
        text
    );
    assert!(saw_exit, "环境探测命令退出后应在超时内收到 Exited 事件");
}

/// spawn_command 将 PtyCommand.cwd 传给子进程工作目录。
///
/// 数据构造（含关键数值推导）：
///   cwd = std::env::temp_dir()/multisoul-pty-cwd-{pid}
///   command = /bin/sh -c "printf '%s' \"$PWD\""
///   cols/rows = 120×24，120 列足够容纳 temp cwd 路径，避免换行影响断言
///
/// 执行过程：
///   1. 创建 cwd 目录
///   2. PtyCommand::new("/bin/sh").args(...).cwd(cwd)
///   3. spawn_command → CommandBuilder::cwd(cwd) → 子进程从 cwd 启动
///   4. 收集 Output 直到 Exited
///
/// 预期结果：
///   - 正断言：输出包含 canonical cwd
///   - 负断言：输出不应等于当前测试进程工作目录
#[test]
fn test_spawn_command_uses_configured_cwd() {
    let cwd = std::env::temp_dir().join(format!("multisoul-pty-cwd-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cwd);
    std::fs::create_dir(&cwd).expect("test should create cwd dir");
    let canonical_cwd = cwd.canonicalize().expect("cwd should canonicalize");
    let process_cwd = std::env::current_dir().expect("test should read process cwd");
    let (tx, rx) = unbounded();
    let command = PtyCommand::new("/bin/sh")
        .args(["-c", "printf '%s' \"$PWD\""])
        .cwd(canonical_cwd.clone());

    let _pty =
        PtyHandle::spawn_command(command, 120, 24, tx).expect("spawn cwd command should work");

    let mut acc = Vec::new();
    let mut saw_exit = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(PtyEvent::Output(bytes)) => acc.extend_from_slice(&bytes),
            Ok(PtyEvent::Exited { .. }) => {
                saw_exit = true;
                break;
            }
            Err(_) => {}
        }
    }
    let _ = std::fs::remove_dir_all(&cwd);

    let text = String::from_utf8_lossy(&acc);
    assert!(
        saw_exit,
        "cwd 探测命令执行结束后应在 3 秒内收到 Exited 事件"
    );
    assert!(
        text.contains(canonical_cwd.to_string_lossy().as_ref()),
        "子进程 PWD 应包含 configured cwd {:?}，实际输出: {:?}",
        canonical_cwd,
        text
    );
    assert!(
        !text.trim().eq(process_cwd.to_string_lossy().as_ref()),
        "子进程不应继续使用测试进程 cwd {:?}，说明 CommandBuilder::cwd 未生效",
        process_cwd
    );
}

/// zsh login shell：先读取 .zprofile 补 PATH，再执行 .zshrc 中的 zoxide 初始化。
///
/// 数据构造（含关键数值推导）：
///   zdotdir = temp/multisoul-zsh-login-{pid}
///   PATH = /usr/bin:/bin:/usr/sbin:/sbin（不包含 zdotdir/bin）
///   fake zoxide = zdotdir/bin/zoxide，输出 `printf PROFILE_ZOXIDE_OK\n`
///   .zprofile: export PATH="$ZDOTDIR/bin:$PATH" → zoxide 变为可查找
///   .zshrc: eval "$(zoxide init zsh)"; exit → 若未读 .zprofile 会报 command not found
///
/// 执行过程：
///   1. 创建隔离 ZDOTDIR、fake zoxide、.zprofile、.zshrc
///   2. spawn_command(/bin/zsh -l, ZDOTDIR=zdotdir, PATH=最小系统 PATH)
///   3. zsh -l 读取 .zprofile → PATH 增加 zdotdir/bin
///   4. zsh 读取 .zshrc → 找到 fake zoxide → eval 后输出 PROFILE_ZOXIDE_OK → exit
///
/// 预期结果：
///   - 正断言：输出包含 PROFILE_ZOXIDE_OK，证明 .zprofile 的 PATH 在 .zshrc 前生效
///   - 负断言：输出不应包含 "command not found"，否则复现 GUI 环境下 zoxide 缺失
///   - 正断言：zsh 在 3 秒内退出，证明 .zshrc 的 exit 被执行
#[test]
fn test_zsh_login_shell_loads_profile_path_before_zshrc_zoxide_init() {
    use std::os::unix::fs::PermissionsExt;

    let zdotdir = std::env::temp_dir().join(format!("multisoul-zsh-login-{}", std::process::id()));
    let bin_dir = zdotdir.join("bin");
    let zoxide_path = bin_dir.join("zoxide");
    let _ = std::fs::remove_dir_all(&zdotdir);
    std::fs::create_dir_all(&bin_dir).expect("test should create isolated zsh bin dir");
    std::fs::write(
        &zoxide_path,
        "#!/bin/sh\nprintf 'printf PROFILE_ZOXIDE_OK\\n'\n",
    )
    .expect("test should write fake zoxide executable");
    let mut permissions = std::fs::metadata(&zoxide_path)
        .expect("fake zoxide metadata should exist")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&zoxide_path, permissions).expect("fake zoxide should be executable");
    std::fs::write(
        zdotdir.join(".zprofile"),
        "export PATH=\"$ZDOTDIR/bin:$PATH\"\n",
    )
    .expect("test should write .zprofile");
    std::fs::write(
        zdotdir.join(".zshrc"),
        "eval \"$(zoxide init zsh)\"\nexit\n",
    )
    .expect("test should write .zshrc");

    let (tx, rx) = unbounded();
    let command = PtyCommand::new("/bin/zsh")
        .arg("-l")
        .env("ZDOTDIR", zdotdir.to_string_lossy())
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");

    let _pty = PtyHandle::spawn_command(command, 120, 24, tx)
        .expect("spawn zsh login shell command should work");

    let mut acc = Vec::new();
    let mut saw_exit = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(PtyEvent::Output(bytes)) => acc.extend_from_slice(&bytes),
            Ok(PtyEvent::Exited { .. }) => {
                saw_exit = true;
                break;
            }
            Err(_) => {}
        }
    }
    let _ = std::fs::remove_dir_all(&zdotdir);

    let text = String::from_utf8_lossy(&acc);
    assert!(
        text.contains("PROFILE_ZOXIDE_OK"),
        "zsh -l 应先读取 .zprofile 补 PATH，再让 .zshrc 找到 zoxide；实际输出: {:?}",
        text
    );
    assert!(
        !text.contains("command not found"),
        "zsh login shell 不应在 .zshrc 中找不到 zoxide；实际输出: {:?}",
        text
    );
    assert!(saw_exit, "隔离 .zshrc 执行 exit 后，zsh 应在 3 秒内退出");
}

/// `env_remove` 必须在 spawn 时从子进程环境删掉父进程同名变量。
///
/// 数据构造：
///   父进程 `MULTISOUL_NODE_ID=424`，模拟卡片终端里启动的 App 继承了来源节点。
///   被移除的命令：`/bin/sh -c 'printf %s "${MULTISOUL_NODE_ID-unset}"'`
///   对照命令不调用 `env_remove`，仍应看到 424。
///
/// 执行过程：
///   1. 设置父进程变量并记下原值，测试结束恢复。
///   2. spawn 带 `env_remove("MULTISOUL_NODE_ID")` 的命令，读到退出。
///   3. spawn 不移除该变量的对照命令。
///
/// 预期结果：
///   - 正断言：移除后子进程输出 `unset`。
///   - 负断言：不移除时子进程输出 `424`，证明变量确实从父进程继承，是 `env_remove` 把它删掉的。
#[test]
fn test_spawn_command_env_remove_drops_inherited_node_identity() {
    const KEY: &str = "MULTISOUL_NODE_ID";
    let previous = std::env::var(KEY).ok();
    struct Restore<'a> {
        key: &'a str,
        previous: Option<String>,
    }
    impl Drop for Restore<'_> {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }
    let _restore = Restore { key: KEY, previous };
    std::env::set_var(KEY, "424");

    let removed = spawn_printed_env(true);
    let inherited = spawn_printed_env(false);

    assert_eq!(
        removed, "unset",
        "env_remove 后子进程不得再看到父进程的 MULTISOUL_NODE_ID=424"
    );
    assert_eq!(
        inherited, "424",
        "未 env_remove 时子进程应继承 424，否则本测试没有证明移除生效"
    );
    assert_ne!(removed, inherited, "移除与继承两条路径的输出必须不同");
}

fn spawn_printed_env(remove_node_id: bool) -> String {
    let (tx, rx) = unbounded();
    let mut command =
        PtyCommand::new("/bin/sh").args(["-c", "printf '%s' \"${MULTISOUL_NODE_ID-unset}\""]);
    if remove_node_id {
        command = command.env_remove("MULTISOUL_NODE_ID");
    }
    let _pty = PtyHandle::spawn_command(command, 80, 24, tx).expect("spawn env probe");
    let mut acc = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(PtyEvent::Output(bytes)) => acc.extend_from_slice(&bytes),
            Ok(PtyEvent::Exited { .. }) => break,
            Err(_) => {}
        }
    }
    String::from_utf8_lossy(&acc).replace(['\r', '\n'], "")
}
