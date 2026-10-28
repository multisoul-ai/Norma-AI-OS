use super::*;
use terminal::PtyBackend;

fn original() -> PtyCommand {
    PtyCommand::new("/bin/zsh").args(["-lic", "exec codex -c disable_paste_burst=true --dangerously-bypass-approvals-and-sandbox --dangerously-bypass-hook-trust resume 'original-session'"])
        .cwd("/tmp/original cwd")
        .env("MULTISOUL_AGENT_PROFILE", "codex")
        .env("MULTISOUL_NODE_ID", "7")
        .env(norma::agent_hook::ENV_CONTROL_RUN_ID, "owned-process")
}

/// 场景：宿主接入只改原TUI传输位置，不能改权限、cwd、resume身份或创建额外任务。
/// 构造：原生Codex恢复命令带已有默认权限、Hook身份及带空格cwd，私有socket路径含单引号。
/// 执行：构造server/TUI两条命令并检查保留的原参数与shell引用。
/// 预期：原参数完整保留、原进程身份一致；server不创建thread/turn，不输入额外任务。
#[test]
fn native_relay_launch_preserves_original_command_policy_and_identity() {
    let command = original();
    assert_eq!(
        owned_identity(&command),
        Some((7, "owned-process".into())),
        "只受控原生Codex命令可使用宿主"
    );
    let (server, tui) = launch_commands(
        &command,
        Path::new("/tmp/private/server.sock"),
        Path::new("/tmp/private/q'uote.sock"),
    )
    .unwrap();
    assert_eq!(tui.cwd, command.cwd, "恢复保持原cwd，不规范化符号链接");
    assert_eq!(server.cwd, command.cwd, "同一受控server使用同cwd");
    assert_eq!(tui.env, command.env, "保留原进程Hook身份");
    assert_eq!(server.env, command.env, "server Hook沿用同一拥有者");
    assert_eq!(tui.args[1], "exec codex --remote 'unix:///tmp/private/q'\\''uote.sock' -c disable_paste_burst=true --dangerously-bypass-approvals-and-sandbox --dangerously-bypass-hook-trust resume 'original-session'", "仅在原命令前补私有remote端点");
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
    while Instant::now() < deadline {
        live.drain();
        if host.is_none() {
            host = live.take_native_question_host();
        }
        if let Some(crate::NodeView::Term { snapshot, .. }) = live.view() {
            screen = snapshot.rows.iter().flatten().map(|cell| cell.c).collect();
        }
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
