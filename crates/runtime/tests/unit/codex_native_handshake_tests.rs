use super::*;
use crate::LiveNode;
use std::os::unix::fs::PermissionsExt;

fn original() -> PtyCommand {
    PtyCommand::new("/bin/zsh")
        .args(["-lic", "exec codex -c disable_paste_burst=true"])
        .env("MULTISOUL_AGENT_PROFILE", "codex")
        .env("MULTISOUL_NODE_ID", "7")
        .env(norma::agent_hook::ENV_CONTROL_RUN_ID, "owned-process")
}

/// 在隔离目录里放一个假 `codex`：版本为已支持的 0.162.1；`app-server` 只创建并监听 socket；
/// `--remote` 模拟更新菜单（按 `mode` 决定交互或刷屏），永远不连接中继 socket。
fn command_with_fake_codex(dir: &std::path::Path, remote_body: &str) -> PtyCommand {
    command_with_prelude(dir, remote_body, "")
}

/// `prelude` 在登录 shell 阶段（TUI 启动之前）执行，用来模拟 `.zshrc`/nvm 的输出噪声。
fn command_with_prelude(dir: &std::path::Path, remote_body: &str, prelude: &str) -> PtyCommand {
    let binary = dir.join("codex");
    let shell = dir.join("shell");
    std::fs::write(
        &binary,
        format!(
            r#"#!/bin/sh
case "$*" in
  '--version') printf 'codex-cli 0.162.1\n'; exit 0;;
  *app-server*) for last; do :; done
    exec python3 -c 'import socket,sys,time; s=socket.socket(socket.AF_UNIX); s.bind(sys.argv[1][7:]); s.listen(1); time.sleep(300)' "$last";;
  *--remote*) {remote_body};;
esac
exit 2
"#
        ),
    )
    .unwrap();
    std::fs::write(
        &shell,
        format!(
            "#!/bin/sh\n{prelude}\nexport PATH=\"$NORMA_CODEX_TEST_BIN:$PATH\"\nexec /bin/zsh -fc \"$2\"\n"
        ),
    )
    .unwrap();
    for path in [&binary, &shell] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut command = original().env("NORMA_CODEX_TEST_BIN", dir.to_string_lossy());
    command.program = shell.to_string_lossy().into_owned();
    command
}

fn screen_of(live: &mut TermLive) -> String {
    live.drain();
    match live.view() {
        Some(crate::NodeView::Term { snapshot, .. }) => {
            snapshot.rows.iter().flatten().map(|cell| cell.c).collect()
        }
        _ => String::new(),
    }
}

fn wait_for(live: &mut TermLive, needle: &str, secs: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        if screen_of(live).contains(needle) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

/// 场景：原生 Codex 先弹出交互菜单（如更新提示），握手完成前用户必须能用键盘操作它。
/// 构造：假 codex 的 `--remote` 打印 MENU，读取一行输入后回显 GOT:<输入>，且不连接中继。
/// 执行：生产 prepare 启动；首屏出现 MENU 后发送 `x\r`。
/// 预期：输入到达原生 PTY（屏幕出现 GOT:x），而不是被「Agent 正在准备」拒绝；kill 后进程回收。
#[test]
fn menu_before_handshake_receives_keyboard_input() {
    let dir = tempfile::tempdir().unwrap();
    let command = command_with_fake_codex(
        dir.path(),
        r#"printf 'MENU\r\n'; IFS= read -r line; printf 'GOT:%s\r\n' "$line"; exec sleep 300"#,
    );
    let control = Arc::new(LaunchControl::default());
    let mut live = prepare(1, command, 100, 24, control.clone())
        .unwrap()
        .into_live()
        .unwrap();
    assert!(wait_for(&mut live, "MENU", 5), "首屏必须出现菜单");
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut sent = false;
    while !sent && Instant::now() < deadline {
        sent = live.input(crate::NodeInput::Text("x\r".into())).is_ok();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(sent, "握手完成前，菜单可见后输入必须可达原生 PTY");
    assert!(wait_for(&mut live, "GOT:x", 3), "原生 PTY 必须收到输入");
    let owned = control.owned_process_ids();
    live.kill();
    let deadline = Instant::now() + Duration::from_secs(3);
    let exists = |pid: &u32| unsafe { libc::kill(*pid as i32, 0) == 0 };
    while owned.iter().any(exists) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !owned.is_empty() && owned.iter().all(|pid| !exists(pid)),
        "{owned:?}"
    );
}

fn limits(idle_ms: u64, cap_ms: u64) -> deferred::Limits {
    deferred::Limits {
        idle: Duration::from_millis(idle_ms),
        cap: Duration::from_millis(cap_ms),
    }
}

fn pids_gone(pids: &[u32]) -> bool {
    let exists = |pid: &u32| unsafe { libc::kill(*pid as i32, 0) == 0 };
    let deadline = Instant::now() + Duration::from_secs(3);
    while pids.iter().any(exists) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    !pids.is_empty() && pids.iter().all(|pid| !exists(pid))
}

/// 场景：原生 TUI 一直刷屏但从不连接中继——这是真正的启动卡死，不能靠持续输出续命。
/// 构造：假 codex 的 `--remote` 每 50ms 输出一行；空闲预算 2.5s，上限 8s；全程无用户输入。
/// 执行：prepare_with 启动并等待，观察 controller 与屏幕。
/// 预期：约 2.5s 后启动超时：屏幕出现「Codex 启动失败」与 `startup timed out`，
///       controller 取消，server 与 TUI 两个自有进程都被回收。
#[test]
fn flooding_output_without_connection_still_times_out_and_cleans_up() {
    let dir = tempfile::tempdir().unwrap();
    let command = command_with_fake_codex(
        dir.path(),
        r#"while :; do printf 'tick\r\n'; sleep 0.05; done"#,
    );
    let control = Arc::new(LaunchControl::default());
    let mut live = prepare_with(1, command, 100, 24, control.clone(), limits(2500, 8000))
        .unwrap()
        .into_live()
        .unwrap();
    assert!(wait_for(&mut live, "tick", 3), "TUI 必须已刷屏");
    let owned = control.owned_process_ids();
    assert_eq!(owned.len(), 2, "超时前应拥有 server 与 TUI：{owned:?}");
    let started = Instant::now();
    assert!(
        wait_for(&mut live, "startup timed out", 4),
        "必须报告启动超时"
    );
    assert!(
        started.elapsed() < Duration::from_millis(4000),
        "持续输出不得把等待延长到空闲预算之外：{:?}",
        started.elapsed()
    );
    assert!(control.is_cancelled(), "超时必须取消 controller");
    assert!(
        pids_gone(&owned),
        "超时后 server 与 TUI 必须被回收：{owned:?}"
    );
}

/// 场景：用户正在操作菜单（持续有键盘输入）时不应被超时打断；停手后仍按空闲预算超时。
/// 构造：假 codex 回显收到的行；空闲预算 2.5s（留出启动耗时），上限 12s；每 400ms 发一次输入，持续 3.5s（> 2.5s）。
/// 执行：输入期间检查 controller 未取消；停止输入后等待超时。
/// 预期：输入期间不取消；停手后约 2.5s 超时并取消，进程回收。
#[test]
fn keyboard_activity_extends_deadline_and_idle_then_times_out() {
    let dir = tempfile::tempdir().unwrap();
    let command = command_with_fake_codex(
        dir.path(),
        r#"printf 'MENU\r\n'; while IFS= read -r line; do printf 'GOT:%s\r\n' "$line"; done"#,
    );
    let control = Arc::new(LaunchControl::default());
    let mut live = prepare_with(1, command, 100, 24, control.clone(), limits(2500, 12000))
        .unwrap()
        .into_live()
        .unwrap();
    assert!(wait_for(&mut live, "MENU", 5), "首屏必须出现菜单");
    let owned = control.owned_process_ids();
    assert_eq!(owned.len(), 2, "{owned:?}");
    let active_until = Instant::now() + Duration::from_millis(3500);
    let mut n = 0;
    while Instant::now() < active_until {
        assert!(
            live.input(crate::NodeInput::Text(format!("k{n}\r")))
                .is_ok(),
            "菜单可见后输入可达"
        );
        n += 1;
        std::thread::sleep(Duration::from_millis(400));
        live.drain();
        assert!(
            !control.is_cancelled(),
            "用户仍在操作时不得超时（第 {n} 次输入后）"
        );
    }
    assert!(
        wait_for(&mut live, "startup timed out", 4),
        "停手后必须按空闲预算超时"
    );
    assert!(control.is_cancelled());
    assert!(pids_gone(&owned), "{owned:?}");
}

/// 场景：绝对上限——即使一直有输入，也不能无限延长启动等待。
/// 构造：空闲预算 2s、上限 4s（留出假 server 启动耗时）；每 300ms 发一次输入。
/// 预期：约 4s 后仍会超时取消，且晚于空闲预算（说明输入确实续了期）。
#[test]
fn hard_cap_bounds_wait_even_with_continuous_input() {
    let dir = tempfile::tempdir().unwrap();
    let command = command_with_fake_codex(
        dir.path(),
        r#"printf 'MENU\r\n'; while IFS= read -r line; do printf 'GOT:%s\r\n' "$line"; done"#,
    );
    let control = Arc::new(LaunchControl::default());
    let mut live = prepare_with(1, command, 100, 24, control.clone(), limits(2000, 4000))
        .unwrap()
        .into_live()
        .unwrap();
    assert!(wait_for(&mut live, "MENU", 5));
    let started = Instant::now();
    let deadline = Instant::now() + Duration::from_secs(8);
    while !control.is_cancelled() && Instant::now() < deadline {
        let _ = live.input(crate::NodeInput::Text("k\r".into()));
        std::thread::sleep(Duration::from_millis(300));
        live.drain();
    }
    assert!(
        control.is_cancelled(),
        "上限到达后必须取消，不能被输入无限续命"
    );
    assert!(
        started.elapsed() > Duration::from_millis(1500),
        "连续输入应把超时推过空闲预算(2s)：{:?}",
        started.elapsed()
    );
}

/// 场景：握手完成前用户关闭卡片，不得遗留 server/TUI 子进程。
/// 构造：假 codex 显示菜单后挂起。预期：kill 后 controller 取消、两个进程回收。
#[test]
fn closing_card_during_prompt_reclaims_server_and_tui() {
    let dir = tempfile::tempdir().unwrap();
    let command = command_with_fake_codex(dir.path(), r#"printf 'MENU\r\n'; exec sleep 300"#);
    let control = Arc::new(LaunchControl::default());
    let mut live = prepare(1, command, 100, 24, control.clone())
        .unwrap()
        .into_live()
        .unwrap();
    assert!(wait_for(&mut live, "MENU", 5));
    let owned = control.owned_process_ids();
    assert_eq!(owned.len(), 2, "server 与 TUI 两个自有进程：{owned:?}");
    live.kill();
    assert!(control.is_cancelled());
    assert!(pids_gone(&owned), "{owned:?}");
}

/// 场景：登录 shell 在 TUI 启动前输出噪声（.zshrc、nvm、motd）；此时用户看到的不是菜单，
///       按键会排在 PTY 行缓冲里，菜单一出现就被默认项（Update now）消费——输入门控不得因此放行。
/// 构造：假 shell 先打印 NOISE 并停顿 1.5 秒，再 exec 包装脚本；假 codex 的 `--remote` 打印 MENU 后读一行。
/// 执行：看到 NOISE 后立即输入；再等 MENU 出现后输入。
/// 预期：NOISE 期间输入被拒绝（门控未开）；MENU 出现后输入可达并被回显。
#[test]
fn shell_noise_before_tui_does_not_open_input_gate() {
    let dir = tempfile::tempdir().unwrap();
    let command = command_with_prelude(
        dir.path(),
        r#"printf 'MENU\r\n'; IFS= read -r line; printf 'GOT:%s\r\n' "$line"; exec sleep 300"#,
        "printf 'NOISE\\r\\n'; sleep 1.5",
    );
    let control = Arc::new(LaunchControl::default());
    let mut live = prepare(1, command, 100, 24, control.clone())
        .unwrap()
        .into_live()
        .unwrap();
    assert!(wait_for(&mut live, "NOISE", 5), "必须先看到噪声");
    assert!(
        live.input(crate::NodeInput::Text("x\r".into())).is_err(),
        "TUI 尚未出画面，噪声输出不得放行输入"
    );
    assert!(wait_for(&mut live, "MENU", 6), "随后菜单出现");
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut sent = false;
    while !sent && Instant::now() < deadline {
        sent = live.input(crate::NodeInput::Text("y\r".into())).is_ok();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(sent, "菜单出现后输入应可达");
    assert!(
        wait_for(&mut live, "GOT:y", 3),
        "菜单只应收到菜单出现后的输入"
    );
    assert!(
        !screen_of(&mut live).contains("GOT:x"),
        "噪声期间的按键不得到达 TUI"
    );
    live.kill();
}

/// 场景：输入门控只认「TUI 启动标记之后的输出」；标记可能被拆在两次读取里。
/// 构造：对 Preparation 依次喂入 shell 噪声、被拆开的标记、标记后的画面。
/// 预期：噪声与「仅标记」都不放行；标记（含跨块）之后出现任意字节才放行；放行后保持放行。
#[test]
fn input_gate_requires_tui_start_marker_then_output() {
    let gate = |chunks: &[&[u8]]| {
        let backend = deferred::DeferredPty::with_limits(
            Arc::new(LaunchControl::default()),
            80,
            24,
            limits(1000, 5000),
        );
        let slot = backend.preparation();
        for chunk in chunks {
            slot.observe_output(chunk);
        }
        slot.accepts_input()
    };
    let marker = deferred::TUI_START_MARKER;
    assert!(!gate(&[]), "没有输出时不放行");
    assert!(!gate(&[b"nvm: using node v22\r\n"]), "shell 噪声不放行");
    assert!(!gate(&[marker]), "仅标记（TUI 还没画任何内容）不放行");
    assert!(gate(&[marker, b"MENU"]), "标记之后的输出放行");
    assert!(
        gate(&[&[b"noise".as_slice(), marker, b"M"].concat()]),
        "同一块里标记之后还有字节也放行"
    );
    let (head, tail) = marker.split_at(7);
    assert!(gate(&[head, tail, b"M"]), "标记被拆成两块仍能识别");
    assert!(!gate(&[head, tail]), "拆开的标记本身同样不放行");
    assert!(!gate(&[b"x\x1b]9999;other\x07 menu"]), "其他 OSC 不是标记");
}

/// 场景：握手期间（菜单可见、尚未连接中继）用户拖拽缩放卡片；原生 TUI 必须同步到新尺寸，
///       否则菜单按旧尺寸绘制而本地网格已变，出现错位。
/// 构造：假 codex 的 `--remote` 打印 MENU，并在收到 SIGWINCH 时回显 `SIZE:<rows> <cols>`；
///       初始 100×24。
/// 执行：MENU 可见后将卡片缩放为 120×30。
/// 预期：原生 PTY 收到 SIGWINCH，回显 `SIZE:30 120`；该进程始终没有连接中继（握手仍未完成）。
#[test]
fn resize_during_handshake_reaches_native_tui() {
    let dir = tempfile::tempdir().unwrap();
    let command = command_with_fake_codex(
        dir.path(),
        r#"printf 'MENU\r\n'; trap 'printf "SIZE:%s\r\n" "$(stty size)"' WINCH; while :; do sleep 0.1; done"#,
    );
    let control = Arc::new(LaunchControl::default());
    let mut live = prepare(1, command, 100, 24, control.clone())
        .unwrap()
        .into_live()
        .unwrap();
    assert!(wait_for(&mut live, "MENU", 5), "首屏必须出现菜单");
    live.resize(crate::NodeResize::TermGrid {
        cols: 120,
        rows: 30,
    })
    .unwrap();
    assert!(
        wait_for(&mut live, "SIZE:30 120", 4),
        "握手期间的缩放必须到达原生 TUI，屏幕={}",
        screen_of(&mut live)
    );
    live.kill();
}

/// 场景：握手期间用户退出原生 TUI（如在菜单里按 Ctrl+C），应保留 TUI 真实的退出码，
///       不能被启动线程补发的 `Exited{code:1}` 覆盖。
/// 构造：假 codex 的 `--remote` 打印 MENU 后以退出码 7 退出。
/// 执行：等待卡片进入已退出状态，再多等 600ms 让可能的重复事件被处理。
/// 预期：状态是 Exited{code: Some(7)}，且不出现「Codex 启动失败」（这不是启动失败）。
#[test]
fn tui_exit_during_handshake_keeps_its_own_exit_code() {
    let dir = tempfile::tempdir().unwrap();
    let command = command_with_fake_codex(dir.path(), "printf 'MENU\\r\\n'; sleep 0.5; exit 7");
    let control = Arc::new(LaunchControl::default());
    let mut live = prepare(1, command, 100, 24, control.clone())
        .unwrap()
        .into_live()
        .unwrap();
    let status = |live: &mut TermLive| {
        live.drain();
        live.terminal_status()
    };
    let deadline = Instant::now() + Duration::from_secs(6);
    while !matches!(
        status(&mut live),
        Some(terminal::TerminalStatus::Exited { .. })
    ) && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(600));
    assert!(
        matches!(
            status(&mut live),
            Some(terminal::TerminalStatus::Exited { code: Some(7) })
        ),
        "应保留真实退出码 7：{:?}",
        status(&mut live)
    );
    assert!(
        !screen_of(&mut live).contains("Codex 启动失败"),
        "TUI 自己退出不是启动失败"
    );
}
