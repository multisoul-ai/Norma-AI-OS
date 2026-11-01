use super::*;
use terminal::PtyBackend;

fn original() -> PtyCommand {
    PtyCommand::new("/bin/zsh").args(["-lic", "exec codex -c disable_paste_burst=true --dangerously-bypass-approvals-and-sandbox --dangerously-bypass-hook-trust resume 'original-session'"])
        .cwd("/tmp/original cwd")
        .env("MULTISOUL_AGENT_PROFILE", "codex")
        .env("MULTISOUL_NODE_ID", "7")
        .env(norma::agent_hook::ENV_CONTROL_RUN_ID, "owned-process")
}

/// 场景：宿主接入只改原TUI传输位置，不能改权限语义、cwd、resume身份或创建额外任务。
/// 构造：原生Codex恢复命令带YOLO权限、Hook身份及带空格cwd，私有socket路径含单引号。
/// 执行：构造server/TUI两条命令并检查保留的原参数与shell引用。
/// 预期：YOLO从远程TUI移交relay（TUI带权限覆盖时Codex拒绝/resume与fork）；其余原参数完整保留。
#[test]
fn native_relay_launch_preserves_original_command_policy_and_identity() {
    let command = original();
    assert_eq!(
        owned_identity(&command),
        Some((7, "owned-process".into())),
        "只受控原生Codex命令可使用宿主"
    );
    let (server, tui, full_access) = launch_commands(
        &command,
        Path::new("/tmp/private/server.sock"),
        Path::new("/tmp/private/q'uote.sock"),
    )
    .unwrap();
    assert!(
        full_access,
        "YOLO权限由relay在thread/start|resume|fork上携带"
    );
    assert_eq!(tui.cwd, command.cwd, "恢复保持原cwd，不规范化符号链接");
    assert_eq!(server.cwd, command.cwd, "同一受控server使用同cwd");
    assert_eq!(tui.env, command.env, "保留原进程Hook身份");
    assert_eq!(server.env, command.env, "server Hook沿用同一拥有者");
    assert_eq!(tui.args[1], "exec codex --remote 'unix:///tmp/private/q'\\''uote.sock' -c disable_paste_burst=true --dangerously-bypass-hook-trust resume 'original-session'", "补私有remote端点并移除远程TUI不接受的权限覆盖");
    assert!(
        !server.args[1].contains("approval") && !server.args[1].contains("sandbox"),
        "server只设默认值不足以保证沙箱（TUI会发本地sandbox），不在server重复设置"
    );
    assert!(
        server.args[1].contains("app-server --listen"),
        "只启动私有协议服务"
    );
    assert!(
        !server.args[1].contains("resume") && !server.args[1].contains("turn/start"),
        "服务进程不另建或恢复用户任务"
    );
    let mut external = command.clone();
    external.env.clear();
    assert!(
        owned_identity(&external).is_none(),
        "无拥有者身份不能静默接管"
    );
}

/// 场景：非YOLO原命令接入宿主时，不能被升级成YOLO。
/// 构造：不带跳过审批参数的原生Codex命令，以及参数名只是前缀相近的命令。
/// 执行：构造server/TUI两条命令。
/// 预期：不开启relay全权限注入；TUI参数仅补remote端点，原审批与沙箱交由用户配置。
#[test]
fn native_relay_launch_never_escalates_non_yolo_command() {
    for script in [
        "exec codex -c disable_paste_burst=true resume 'original-session'",
        "exec codex --dangerously-bypass-approvals-and-sandbox-later",
    ] {
        let mut command = original();
        command.args[1] = script.into();
        let (server, tui, full_access) = launch_commands(
            &command,
            Path::new("/tmp/private/server.sock"),
            Path::new("/tmp/private/native.sock"),
        )
        .unwrap();
        assert!(!full_access, "{script} 不能被升级为YOLO");
        assert_eq!(
            tui.args[1],
            script.replacen(
                "exec codex ",
                "exec codex --remote 'unix:///tmp/private/native.sock' ",
                1
            ),
            "非YOLO命令原样保留"
        );
        assert!(!server.args[1].contains("approval") && !server.args[1].contains("sandbox"));
    }
}

/// 场景：异步准备尚未附着时，输入不能排队到未知界面；关闭阻止迟到进程进入。
/// 构造：空DeferredPty与其LaunchControl，80×24初始grid，无任何子进程。
/// 执行：尝试输入、改100×30、关闭，再尝试输入及由同control启动/bin/cat。
/// 预期：输入立即拒绝且无队列，保留新尺寸；控制器已取消并拒绝迟到spawn。
#[test]
fn preparing_native_terminal_rejects_input_and_cancels_late_spawn() {
    let control = Arc::new(LaunchControl::default());
    let mut backend = deferred::DeferredPty::new(control.clone(), 80, 24);
    assert!(
        backend.write(b"never queue this").is_err(),
        "准备期间不缓存输入"
    );
    backend.resize(100, 30).unwrap();
    assert_eq!(backend.dimensions(), (100, 30), "后台附着时使用最新grid");
    backend.kill();
    assert!(control.is_cancelled(), "关闭拥有者取消准备任务");
    assert!(backend.write(b"late").is_err(), "关闭后仍不能输入");
    let (sender, _receiver) = crossbeam_channel::bounded(1);
    assert!(
        control
            .spawn_pty(PtyCommand::new("/bin/cat"), 80, 24, sender)
            .is_err(),
        "取消后不能漏出迟到子进程"
    );
}

/// 场景：未经验证或探测失败的 Codex 不能因不支持 remote/unix 而失去原卡片。
/// 构造：隔离 CLI 和登录 shell 替身，记录参数；覆盖旧版、未来版及探测失败。
/// 执行：生产 prepare 启动带 resume 的原 TUI，等待提示并显式关闭自有进程。
/// 预期：仅探测版本和执行原命令，无 app-server/remote，原 cwd、resume 与参数保留。
#[test]
fn unsupported_codex_preserves_original_tui_and_resume() {
    use crate::LiveNode;
    use std::os::unix::fs::PermissionsExt;
    for version in [
        "codex-cli 0.100.0",
        "codex-cli 0.163.0",
        "codex-cli 99.0.0",
        "probe-failed",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("codex");
        let shell = dir.path().join("shell");
        let log = dir.path().join("calls.txt");
        let cwd_log = dir.path().join("cwd.txt");
        std::fs::write(
            &binary,
            r#"#!/bin/sh
printf '%s\n' "$*" >> "$NORMA_CODEX_TEST_LOG"
if [ "$1" = '--version' ]; then
    [ "$NORMA_CODEX_TEST_VERSION" = 'probe-failed' ] && exit 1
    printf '%s\n' "$NORMA_CODEX_TEST_VERSION"; exit 0
fi
case "$*" in *app-server*|*--remote*) exit 2;; esac
pwd > "$NORMA_CODEX_TEST_CWD"
printf 'ORIGINAL_CODEX_READY\r\n'
while IFS= read -r line; do printf 'CHILD_ACK:%s\r\n' "$line"; done
"#,
        )
        .unwrap();
        std::fs::write(
            &shell,
            "#!/bin/sh\nexport PATH=\"$NORMA_CODEX_TEST_BIN:$PATH\"\nexec /bin/zsh -fc \"$2\"\n",
        )
        .unwrap();
        for path in [&binary, &shell] {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let mut command = original().with_cwd(Some(dir.path()));
        command.program = shell.to_string_lossy().into_owned();
        command = command
            .env("NORMA_CODEX_TEST_BIN", dir.path().to_string_lossy())
            .env("NORMA_CODEX_TEST_LOG", log.to_string_lossy())
            .env("NORMA_CODEX_TEST_CWD", cwd_log.to_string_lossy())
            .env("NORMA_CODEX_TEST_VERSION", version);
        let control = Arc::new(LaunchControl::default());
        let mut live = prepare(1, command, 100, 24, control.clone())
            .unwrap()
            .into_live()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(4);
        let mut screen = String::new();
        let mut host = None;
        while Instant::now() < deadline {
            live.drain();
            if host.is_none() {
                host = live.take_native_question_host();
            }
            if let Some(crate::NodeView::Term { snapshot, .. }) = live.view() {
                screen = snapshot.rows.iter().flatten().map(|cell| cell.c).collect();
            }
            if screen.contains("ORIGINAL_CODEX_READY") && !live.native_question_host_pending() {
                break;
            }
            if control.is_cancelled() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let input = live.input(crate::NodeInput::Text("FALLBACK_INPUT\r".into()));
        let input_deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < input_deadline && !screen.contains("CHILD_ACK:FALLBACK_INPUT") {
            live.drain();
            if let Some(crate::NodeView::Term { snapshot, .. }) = live.view() {
                screen = snapshot.rows.iter().flatten().map(|cell| cell.c).collect();
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let pid = live.child_pid();
        live.kill();
        let deadline = Instant::now() + Duration::from_secs(3);
        let alive = || pid.is_some_and(|pid| unsafe { libc::kill(pid as i32, 0) == 0 });
        while alive() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            input.is_ok() && screen.contains("CHILD_ACK:FALLBACK_INPUT"),
            "原 TUI 实际接收输入并回显"
        );
        assert!(pid.is_some() && !alive(), "关闭必须回收实际子进程");
        assert_eq!(
            std::fs::read_to_string(cwd_log).unwrap().trim(),
            dir.path().canonicalize().unwrap().to_str().unwrap(),
            "原 cwd 保留"
        );
        let calls = std::fs::read_to_string(log).unwrap();
        assert!(
            screen.contains("ORIGINAL_CODEX_READY"),
            "{version} 的原 TUI 必须可用：{screen}; calls={calls}"
        );
        assert!(host.is_none(), "未经验证的版本不开放代答");
        assert_eq!(calls.lines().collect::<Vec<_>>(), vec!["--version", "-c disable_paste_burst=true --dangerously-bypass-approvals-and-sandbox --dangerously-bypass-hook-trust resume original-session"], "探测后完整保留原命令，不启动不支持的传输");
    }
}

/// 场景：真实本机Codex CLI通过生产后台宿主启动，测试不发模型任务，仅核验准备与回收。
/// 构造：沿用已信任仓库cwd、新建私有server/TUI，禁用本次测试Hook，不修改用户配置。
/// 执行：启动TermLive、等待同进程宿主与首屏、验证已核验版本仍可观察，再kill并等待退出。
/// 预期：原TUI有独立PID、无额外turn，显式回收控制器拥有的server与TUI。
#[test]
#[ignore = "requires installed Codex 0.162.1; launches isolated native processes"]
fn native_codex_real_launch_and_cleanup() {
    use crate::LiveNode;
    let cwd = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut command = original().with_cwd(Some(&cwd));
    command.args[1] = "exec codex -c disable_paste_burst=true --dangerously-bypass-approvals-and-sandbox --dangerously-bypass-hook-trust --disable hooks --no-alt-screen".into();
    let control = Arc::new(LaunchControl::default());
    let mut live = prepare(1, command, 100, 32, control.clone())
        .unwrap()
        .into_live()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(25);
    let mut host = None;
    let mut screen = String::new();
    let mut skipped = 0u8;
    let mut last_down = None;
    while Instant::now() < deadline {
        live.drain();
        if host.is_none() {
            host = live.take_native_question_host();
        }
        if let Some(crate::NodeView::Term { snapshot, .. }) = live.view() {
            screen = snapshot.rows.iter().flatten().map(|cell| cell.c).collect();
        }
        skip_update_prompt(&mut live, &screen, &mut skipped, &mut last_down);
        if host.is_some() && (screen.contains("Ask Codex") || screen.contains("model:")) {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let pid = live.child_pid();
    let available =
        matches!(&host, Some(crate::live::NativeQuestionHost::Codex(host)) if host.is_available());
    let initialized = host.is_some();
    let owned = control.owned_process_ids();
    live.kill();
    drop(host);
    let deadline = Instant::now() + Duration::from_secs(3);
    let exists = |pid: &u32| unsafe { libc::kill(*pid as i32, 0) == 0 };
    while owned.iter().any(exists) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(initialized, "真实原TUI必须完成relay握手，首屏={screen}");
    assert!(available, "实测版本通过原initialize回包核验，首屏={screen}");
    assert!(pid.is_some(), "后台准备成功后暴露原TUI PID");
    assert!(control.is_cancelled(), "kill必须统一取消私有server和原TUI");
    assert_eq!(owned.len(), 2, "必须观察到本次server和原TUI两个独立进程");
    assert!(
        owned.iter().all(|pid| !exists(pid)),
        "两个实际PID都必须退出，不能只检查取消标志：{owned:?}"
    );
}

/// 场景：原生 Codex 已启动后，用户通过 /resume 打开会话选择器。
/// 构造：真实 CLI、独立 server/TUI 与已信任 cwd；禁用 Hook，不发送模型任务。
/// 执行：等待首屏，两次输入 /resume 并用 Esc 返回，再关闭本测试拥有的进程。
/// 预期：两次均显示会话列表并回到原会话；无连接错误，原问答宿主保持可用。
#[test]
#[ignore = "requires installed Codex; opens the native picker without model requests"]
fn native_codex_real_session_picker_after_startup() {
    use crate::{LiveNode, NodeInput};
    let cwd = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut command = original().with_cwd(Some(&cwd));
    command.args[1] =
        "exec codex -c disable_paste_burst=true --disable hooks --no-alt-screen".into();
    let control = Arc::new(LaunchControl::default());
    let mut live = prepare(1, command, 120, 32, control.clone())
        .unwrap()
        .into_live()
        .unwrap();
    let mut host = None;
    let mut screen = String::new();
    let mut stage = 0;
    let mut skipped = 0u8;
    let mut last_down = None;
    let deadline = Instant::now() + Duration::from_secs(25);
    while Instant::now() < deadline {
        live.drain();
        if host.is_none() {
            host = live.take_native_question_host();
        }
        if let Some(crate::NodeView::Term { snapshot, .. }) = live.view() {
            screen = snapshot.rows.iter().flatten().map(|cell| cell.c).collect();
        }
        skip_update_prompt(&mut live, &screen, &mut skipped, &mut last_down);
        if screen.contains("Failed to start TUI session picker") {
            break;
        }
        if stage % 2 == 0 && host.is_some() && screen.contains("Ask Codex") {
            if stage == 4 {
                if matches!(&host, Some(crate::live::NativeQuestionHost::Codex(host)) if !host.input_pending())
                {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
            live.input(NodeInput::Text("/resume".into())).unwrap();
            std::thread::sleep(Duration::from_millis(200));
            live.input(NodeInput::Key(terminal::KeyInput {
                key: terminal::Key::Enter,
                ctrl: false,
                alt: false,
                shift: false,
            }))
            .unwrap();
            stage += 1;
        } else if stage % 2 == 1 && screen.contains("Resume a previous session") {
            live.input(NodeInput::Key(terminal::KeyInput {
                key: terminal::Key::Escape,
                ctrl: false,
                alt: false,
                shift: false,
            }))
            .unwrap();
            stage += 1;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let available =
        matches!(&host, Some(crate::live::NativeQuestionHost::Codex(host)) if host.is_available());
    let input_held =
        matches!(&host, Some(crate::live::NativeQuestionHost::Codex(host)) if host.input_pending());
    live.kill();
    drop(host);
    assert!(
        !screen.contains("Failed to start TUI session picker"),
        "同一原生 TUI 的会话选择器必须能够连接：{screen}"
    );
    assert!(
        stage == 4 && screen.contains("Ask Codex"),
        "必须两次打开真实选择器并回到原会话，stage={stage}：{screen}"
    );
    assert!(available, "关闭辅助选择器不能使原问答宿主失效");
    assert!(!input_held, "Esc 返回原会话后不能遗留选择器输入锁");
    assert!(control.is_cancelled(), "测试必须回收自有 server/TUI");
}

/// 场景：非 Codex 的延迟后端（OpenCode/Cursor）装入后立即可输入，不依赖「首屏可见」门控。
/// 构造：`DeferredPty::new` + 真实 /bin/cat。预期：装入后第一次 write 成功。
#[test]
fn non_codex_deferred_backend_accepts_input_once_installed() {
    let control = Arc::new(LaunchControl::default());
    let mut backend = deferred::DeferredPty::new(control.clone(), 80, 24);
    let (sender, _receiver) = crossbeam_channel::unbounded();
    backend
        .preparation()
        .install(
            control
                .spawn_pty(PtyCommand::new("/bin/cat"), 80, 24, sender)
                .unwrap(),
        )
        .unwrap();
    assert!(backend.write(b"hi\n").is_ok(), "装入后应可输入");
    backend.kill();
}

/// 真实 Codex 若弹出「Update available」菜单（握手前），用 ↓ 选到 Skip 再回车。
/// 绝不盲按回车：只有屏幕确认光标已在「2. Skip」才回车，否则会真的执行 `npm install -g`。
/// ↓ 可能恰好落在 TUI 进入原始模式之前而丢失，所以光标仍在第 1 项时每 500ms 重发一次。
/// `stage`：0 未处理，1 已发 ↓，2 已回车；`last_down` 为上次发 ↓ 的时刻。
fn skip_update_prompt(
    live: &mut TermLive,
    screen: &str,
    stage: &mut u8,
    last_down: &mut Option<Instant>,
) {
    use crate::{LiveNode, NodeInput};
    let key = |key| {
        NodeInput::Key(terminal::KeyInput {
            key,
            ctrl: false,
            alt: false,
            shift: false,
        })
    };
    match *stage {
        0 | 1 if screen.contains("Update available") && screen.contains("› 1.") => {
            let due = last_down.is_none_or(|at| at.elapsed() >= Duration::from_millis(500));
            if due && live.input(key(terminal::Key::Down)).is_ok() {
                *stage = 1;
                *last_down = Some(Instant::now());
            }
        }
        1 if screen.contains("› 2.") => {
            if live.input(key(terminal::Key::Enter)).is_ok() {
                *stage = 2;
            }
        }
        _ => {}
    }
}
