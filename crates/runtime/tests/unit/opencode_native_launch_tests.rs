use super::*;
use crate::runtime::LaunchControl;
use crate::LiveNode;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 场景：未知OpenCode版本不能先创建服务和替代会话，只能保留原生新建/恢复命令。
/// 构造：隔离CLI返回未来版本，记录全部参数；分别使用原新建与resume入口。
/// 执行：生产prepare创建原PTY，等待原CLI提示后关闭所有自有进程。
/// 预期：只有版本探测与原命令，没有serve/attach、session创建或答题宿主。
#[test]
fn native_opencode_unknown_version_preserves_original_launch() {
    for (resume, config) in [
        (None, None),
        (Some("ses_original"), None),
        (None, Some("OPENCODE_TUI_CONFIG")),
        (None, Some("OPENCODE_PURE")),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("opencode.sh");
        let log = dir.path().join("calls.txt");
        std::fs::write(
            &binary,
            r#"#!/bin/sh
printf '%s\n' "$*" >> "$NORMA_OPENCODE_TEST_LOG"
if [ "$1" = '--version' ]; then printf '%s\n' "$NORMA_OPENCODE_TEST_VERSION"; exit 0; fi
printf 'ORIGINAL_OPENCODE_READY\r\n'
while IFS= read -r line; do printf '%s\r\n' "$line"; done
"#,
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut command = agent_command(AgentProfile::OpenCode, None)
            .cwd(dir.path())
            .env("MULTISOUL_AGENT_PROFILE", "opencode")
            .env("MULTISOUL_NODE_ID", "7")
            .env(norma::agent_hook::ENV_CONTROL_RUN_ID, "unknown-owned")
            .env("OPENCODE_BIN", binary.to_string_lossy())
            .env(
                "NORMA_OPENCODE_TEST_VERSION",
                if config.is_some() {
                    "1.19.3"
                } else {
                    "future-version"
                },
            )
            .env("NORMA_OPENCODE_TEST_LOG", log.to_string_lossy());
        if let Some(config) = config {
            command = command.env(config, "preserve-user-value");
        }
        if let Some(session) = resume {
            command.args[1] = format!("exec \"${{OPENCODE_BIN:-opencode}}\" resume '{session}'");
        }
        let control = Arc::new(LaunchControl::default());
        let mut live = prepare_agent_command(1, command, 100, 24, control.clone())
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
            if screen.contains("ORIGINAL_OPENCODE_READY") && !live.native_question_host_pending() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        live.kill();
        control.cancel();
        let calls = std::fs::read_to_string(log).unwrap();
        let original = resume.map(|id| format!("resume {id}")).unwrap_or_default();
        assert_eq!(
            calls.lines().collect::<Vec<_>>(),
            vec!["--version", &original],
            "未知版只能执行原命令，无服务/新session副作用"
        );
        assert!(host.is_none(), "未知版不交付未经验证的宿主");
        assert!(
            screen.contains("ORIGINAL_OPENCODE_READY"),
            "原TUI仍须可用:{screen}"
        );
    }
}

/// 场景：生产默认准备入口为 OpenCode 原 TUI 创建同实例问答宿主，退出回收服务与 TUI。
/// 构造：真实已安装 CLI、隔离数据库与临时 cwd，不发送模型任务，不改变用户配置。
/// 执行：通过 prepare_agent_command 启动并等宿主交付；记录两个实际 PID 后显式关闭。
/// 预期：有原生宿主和 TUI 输出，两个 PID 均退出；不能以普通未附着 TUI 算接入成功。
#[test]
#[ignore = "requires installed OpenCode 1.19.3; no model task"]
fn native_opencode_real_launch_and_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let command = agent_command(AgentProfile::OpenCode, None)
        .cwd(directory.path())
        .env("MULTISOUL_AGENT_PROFILE", "opencode")
        .env("MULTISOUL_NODE_ID", "7")
        .env(norma::agent_hook::ENV_CONTROL_RUN_ID, "isolated-native-run")
        .env(
            "OPENCODE_DB",
            directory.path().join("isolated.db").to_string_lossy(),
        )
        .env("OPENCODE_DISABLE_AUTOUPDATE", "1");
    let control = Arc::new(LaunchControl::default());
    let mut live = prepare_agent_command(1, command, 100, 32, control.clone())
        .unwrap()
        .into_live()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
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
        if matches!(&host, Some(crate::live::NativeQuestionHost::OpenCode(host)) if host.matches_session(host.session_id()))
            && !screen.trim().is_empty()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let foreground_confirmed = matches!(&host, Some(crate::live::NativeQuestionHost::OpenCode(host)) if host.matches_session(host.session_id()));
    let pids = control.owned_process_ids();
    live.kill();
    control.cancel();
    drop(live);
    let deadline = Instant::now() + Duration::from_secs(3);
    let exists = |pid: &u32| unsafe { libc::kill(*pid as i32, 0) == 0 };
    while pids.iter().any(exists) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(host.is_some(), "生产准备必须交付原生宿主，screen={screen}");
    assert!(
        foreground_confirmed,
        "原 TUI 插件必须实际报告当前会话；有 server/attach 输出不算:{screen}"
    );
    assert_eq!(pids.len(), 2, "必须实际拥有 server 和原 TUI 两个进程");
    assert!(
        pids.iter().all(|pid| !exists(pid)),
        "所有自有进程都须实际退出：{pids:?}"
    );
    assert!(!screen.trim().is_empty(), "原 TUI 必须有可见输出");
}
