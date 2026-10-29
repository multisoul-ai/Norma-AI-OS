use super::*;
use crate::LiveNode;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 场景：当前 Cursor 没有能确认前台 session/call 归属的答题接口，旧 store 可迟到。
/// 构造：已检查版本和未知版本的假 CLI；主/子会话共享前台控件，版本本身不证明归属。
/// 执行：从生产 prepare 入口启动原 TUI，等待就绪后关闭自有 PID。
/// 预期：两个版本都保留原生输入，不创建代答宿主或替换 session，因而不能错投旧题答案。
#[test]
fn native_cursor_without_foreground_identity_preserves_original_tui() {
    for supported in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("cursor-fixture.sh");
        let log = directory.path().join("calls.txt");
        std::fs::write(
            &binary,
            format!(
                r#"#!/bin/sh
printf '%s\n' "$*" >> "$NORMA_CURSOR_TEST_LOG"
case "$1" in
--version) printf '%s\n' '{}'; exit 0;;
create-chat) printf '%s\n' 'original-session'; exit 0;;
esac
printf 'CURSOR_NATIVE_READY\r\n'
while IFS= read -r line; do printf '%s\r\n' "$line"; done
"#,
                if supported {
                    "2026.10.18-ea085cb"
                } else {
                    "future-version"
                }
            ),
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let command = agent_command(AgentProfile::Cursor, None)
            .cwd(directory.path())
            .env("MULTISOUL_AGENT_PROFILE", "cursor")
            .env("MULTISOUL_NODE_ID", "7")
            .env(norma::agent_hook::ENV_CONTROL_RUN_ID, "cursor-owned")
            .env("CURSOR_AGENT_BIN", binary.to_string_lossy())
            .env("NORMA_CURSOR_TEST_LOG", log.to_string_lossy())
            .env(
                "CURSOR_CONFIG_DIR",
                directory.path().join("config").to_string_lossy(),
            );
        let control = Arc::new(crate::runtime::LaunchControl::default());
        let mut live = prepare_agent_command(1, command, 100, 24, control.clone())
            .unwrap()
            .into_live()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(6);
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
            if screen.contains("CURSOR_NATIVE_READY") && !live.native_question_host_pending() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let pids = control.owned_process_ids();
        live.kill();
        control.cancel();
        let exit_deadline = Instant::now() + Duration::from_secs(3);
        while pids
            .iter()
            .any(|pid| unsafe { libc::kill(*pid as i32, 0) == 0 })
            && Instant::now() < exit_deadline
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        let calls = std::fs::read_to_string(log).unwrap();
        assert!(
            host.is_none(),
            "没有前台 session/call 证据时不能开放逐键代答"
        );
        assert_eq!(
            calls.lines().collect::<Vec<_>>(),
            vec![""],
            "原 TUI 保留默认会话选择；不探测版本、创建替代会话或注入答题输入"
        );
        assert!(
            screen.contains("CURSOR_NATIVE_READY"),
            "两分支的原TUI都可用:{screen}"
        );
        assert_eq!(pids.len(), 1, "仅一个受控启动进程，准备命令是它的子进程");
        assert!(
            pids.iter()
                .all(|pid| unsafe { libc::kill(*pid as i32, 0) != 0 }),
            "关闭后自有进程已回收"
        );
    }
}

/// 场景：真实已安装Cursor经默认配置和准备入口启动原TUI，关闭回收自有进程；没有可靠前台归属就不创建代答host。
/// 构造：当前仓库cwd与既有Cursor配置，原TUI创建唯一新会话；不加wrapper或模型请求。
/// 执行：等待原宿主与真实空prompt，观察store是否已初始化，随后关闭检查原PID退出。
/// 预期：生产默认启动到同一个原session；不把假CLI或后台store单测当真实启动证据。
#[test]
#[ignore = "requires installed Cursor 2026.10.18-ea085cb; no model request"]
fn native_cursor_real_launch_and_cleanup() {
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let command = agent_command(AgentProfile::Cursor, None)
        .cwd(workspace)
        .env("MULTISOUL_AGENT_PROFILE", "cursor")
        .env("MULTISOUL_NODE_ID", "7")
        .env(norma::agent_hook::ENV_CONTROL_RUN_ID, "cursor-real-owned");
    let control = Arc::new(crate::runtime::LaunchControl::default());
    let mut live = prepare_agent_command(1, command, 120, 42, control.clone())
        .unwrap()
        .into_live()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut host = None;
    let mut screen = String::new();

    while Instant::now() < deadline {
        live.drain();
        if host.is_none() {
            host = live.take_native_question_host();
        }
        if let Some(crate::NodeView::Term { snapshot, .. }) = live.view() {
            screen = snapshot
                .rows
                .iter()
                .map(|row| row.iter().map(|cell| cell.c).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n");
        }
        if screen.contains("Plan, search, build anything") || screen.contains("Add a follow-up") {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let pids = control.owned_process_ids();
    live.kill();
    control.cancel();
    let deadline = Instant::now() + Duration::from_secs(3);
    while pids
        .iter()
        .any(|pid| unsafe { libc::kill(*pid as i32, 0) == 0 })
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(host.is_none(), "生产入口不得开放无归属证明的代答:{screen}");
    assert!(
        screen.contains("Plan, search, build anything") || screen.contains("Add a follow-up"),
        "原TUI必须实际就绪，信任/插件准备界面不算:{screen}"
    );
    assert_eq!(pids.len(), 1, "仅启动一个原TUI进程");
    assert!(
        pids.iter()
            .all(|pid| unsafe { libc::kill(*pid as i32, 0) != 0 }),
        "原TUI实际退出:{pids:?}"
    );
    println!("NATIVE_CURSOR_STARTUP pids={pids:?} answer_host=false processes_reaped=true");
}
