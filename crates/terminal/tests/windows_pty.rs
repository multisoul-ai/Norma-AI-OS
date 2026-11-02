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

/// 场景：命令快速退出前产生多屏输出，退出事件不能抢在最后输出之前。
/// 构造：1024条编号记录、唯一尾部标记及exit 7，使用真实ConPTY会话。
/// 执行：按App的drain路径处理协议和输出，仍限定10秒正常收口。
/// 预期：最后一条记录和尾部标记在网格内，退出码准确为7，无Running残留。
#[test]
fn powershell_preserves_tail_output_and_nonzero_exit_status() {
    let command = PtyCommand::new("powershell.exe").args([
        "-NoLogo",
        "-NoProfile",
        "-Command",
        "1..1024 | ForEach-Object { Write-Output ('NORMA_RECORD_' + $_) }; Write-Output 'NORMA_FINAL_OUTPUT'; exit 7",
    ]);
    let mut session = TerminalSession::spawn_command(2, command, 64, 20)
        .expect("必须真实启动大量输出的PowerShell");
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
        text.contains("NORMA_RECORD_1024"),
        "关闭前必须排空最后记录：{text:?}"
    );
    assert!(
        text.contains("NORMA_FINAL_OUTPUT"),
        "最终输出不能被退出事件截断：{text:?}"
    );
    assert_eq!(
        session.status,
        TerminalStatus::Exited { code: Some(7) },
        "必须保留真实退出码，不能仍为Running或把关闭控制台当成正常退出"
    );
}
