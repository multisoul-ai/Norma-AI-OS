#![cfg(windows)]
use std::time::{Duration, Instant};
use terminal::{PtyCommand, PtyEvent, PtyHandle};

/// 场景：Windows发布版终端需要实际通过ConPTY执行PowerShell，不依赖zsh/WSL。
/// 构造：原生PowerShell输出固定标记并退出，64×20终端，既有跨线程输出通道。
/// 执行：创建真实PTY并在10秒总预算内消费输出和退出事件。
/// 预期：输出正文包含唯一标记，子进程正常收口，不能只证明命令字符串生成正确。
#[test]
fn powershell_executes_in_a_real_conpty_and_exits() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let command = PtyCommand::new("powershell.exe").args([
        "-NoLogo",
        "-NoProfile",
        "-Command",
        "Write-Output 'NORMA_WINDOWS_CONPTY_OK'",
    ]);
    let pty = PtyHandle::spawn_command(command, 64, 20, tx).expect("Windows必须实际创建ConPTY");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut bytes = Vec::new();
    let code = loop {
        match rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("ConPTY必须在预算内退出")
        {
            PtyEvent::Output(chunk) => bytes.extend(chunk),
            PtyEvent::Exited { code } => break code,
        }
    };
    drop(pty);
    assert!(
        String::from_utf8_lossy(&bytes).contains("NORMA_WINDOWS_CONPTY_OK"),
        "必须收到真实PowerShell正文，不能用模拟PTY冒充"
    );
    assert_eq!(code, Some(0), "PowerShell子进程必须正常结束");
}
