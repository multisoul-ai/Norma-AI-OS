#![cfg(windows)]
use std::time::{Duration, Instant};
use terminal::{PtyCommand, TerminalSession, TerminalStatus};

/// 场景：发布版终端必须通过ConPTY执行PowerShell，不依赖zsh/WSL。
/// 构造：原生PowerShell输出唯一标记，64×20终端，实际TerminalSession/Grid/PTY。
/// 执行：像App一样drain，真实Grid响应ConPTY继承光标的DSR查询，10秒内等待退出。
/// 预期：可见网格收到标记，状态为正常退出；不能只生成命令或忽略启动协议查询。
#[test]
fn powershell_executes_in_a_real_conpty_and_exits() {
    let command = PtyCommand::new("powershell.exe").args([
        "-NoLogo",
        "-NoProfile",
        "-Command",
        "Write-Output 'NORMA_WINDOWS_CONPTY_OK'",
    ]);
    let mut session =
        TerminalSession::spawn_command(1, command, 64, 20).expect("Windows必须实际创建ConPTY会话");
    let deadline = Instant::now() + Duration::from_secs(10);
    while session.status == TerminalStatus::Running && Instant::now() < deadline {
        session.drain();
        std::thread::sleep(Duration::from_millis(5));
    }
    session.drain();
    let snapshot = session.snapshot();
    let text = snapshot
        .rows
        .iter()
        .map(|row| row.iter().map(|cell| cell.c).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("NORMA_WINDOWS_CONPTY_OK"),
        "必须收到PowerShell实际输出，网格为 {text:?}"
    );
    assert_eq!(
        session.status,
        TerminalStatus::Exited { code: Some(0) },
        "PowerShell必须在10秒内正常结束，不能仍处于Running"
    );
}
