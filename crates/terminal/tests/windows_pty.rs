#![cfg(windows)]
use std::time::{Duration, Instant};
use terminal::{PtyCommand, PtyEvent, PtyHandle, TerminalGrid, TerminalSession, TerminalStatus};

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

/// 场景：主动kill必须仍发布退出事件，不能只丢弃输出后断开通道。
/// 构造：真实PowerShell写ready后睡眠30秒；真实Grid处理ConPTY握手。
/// 执行：确认ready，再按精确PTY持有的进程句柄kill，3秒内接收Exited。
/// 预期：得到Windows TerminateProcess的真实退出码1；通道断开不能冒充退出事件。
#[test]
fn killing_conpty_publishes_its_exit_event() {
    let (output, events) = crossbeam_channel::unbounded();
    let command = PtyCommand::new("powershell.exe").args([
        "-NoLogo",
        "-NoProfile",
        "-Command",
        "[Console]::WriteLine('NORMA_READY_TO_KILL'); Start-Sleep -Seconds 30",
    ]);
    let mut pty = PtyHandle::spawn_command(command, 64, 20, output).expect("真实取消PTY");
    let mut grid = TerminalGrid::new(64, 20);
    let mut bytes = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let event = match events.recv_timeout(
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(5)),
        ) {
            Ok(event) => event,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) if Instant::now() < deadline => {
                if let Some(reply) = grid.expire_synchronized_output() {
                    if !reply.is_empty() {
                        pty.write(&reply)
                            .expect("空闲同步到期也必须回写真实协议响应");
                    }
                }
                continue;
            }
            Err(error) => panic!(
                "必须在取消前完成启动：{error:?}; raw={} bytes; prefix={:02x?}",
                bytes.len(),
                &bytes[..bytes.len().min(128)]
            ),
        };
        match event {
            PtyEvent::Output(chunk) => {
                let reply = grid.advance(&chunk);
                if bytes.len() < 128 {
                    eprintln!(
                        "ConPTY startup raw={:02x?}; reply={:02x?}",
                        &chunk[..chunk.len().min(128 - bytes.len())],
                        reply
                    );
                }
                if !reply.is_empty() {
                    pty.write(&reply).expect("真实协议响应");
                }
                bytes.extend(chunk);
                if String::from_utf8_lossy(&bytes).contains("NORMA_READY_TO_KILL") {
                    break;
                }
            }
            PtyEvent::Exited { code } => panic!("取消之前子进程不能提前退出：{code:?}"),
        }
    }
    pty.kill();
    let deadline = Instant::now() + Duration::from_secs(3);
    let code = loop {
        match events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("kill必须发布Exited，不能只关闭事件通道")
        {
            PtyEvent::Output(_) => {}
            PtyEvent::Exited { code } => break code,
        }
    };
    assert_eq!(code, Some(1), "必须保留精确进程句柄的真实终止状态");
}
