use super::*;
use std::io::{Read, Write};
use std::os::windows::io::AsRawHandle;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use winapi::um::namedpipeapi::{GetNamedPipeInfo, PeekNamedPipe};

/// 场景：调用方的reader失效后，隐藏原始读端不得使旧版ConPTY关闭永久等待。
/// 构造：真实PowerShell持续写入，DSR握手后释放外部reader，用Peek确认输出管道已满。
/// 执行：后台销毁最后master，原3秒预算内观察返回；无消费型探测、无假输出或版本跳过。
/// 预期：满管道前提真实成立，ClosePseudoConsole可完成；精确进程句柄用于清理客户端。
#[test]
fn abandoned_reader_does_not_deadlock_full_pipe_close() {
    let pair = ConPtySystem::default()
        .openpty(PtySize {
            rows: 20,
            cols: 64,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("真实ConPTY");
    let erased: &dyn crate::MasterPty = pair.master.as_ref();
    let master = erased
        .downcast_ref::<ConPtyMasterPty>()
        .expect("Windows master");
    let raw = master
        .inner
        .lock()
        .expect("读端锁")
        .readable
        .as_raw_handle();
    let mut inbound = 0;
    let mut outbound = 0;
    assert_ne!(
        unsafe {
            GetNamedPipeInfo(
                raw as _,
                std::ptr::null_mut(),
                &mut outbound,
                &mut inbound,
                std::ptr::null_mut(),
            )
        },
        0,
        "必须读取真实管道容量"
    );
    let capacity = inbound.max(outbound);
    assert!(capacity > 0, "容量必须可观测，不能猜测64KiB");
    let writer = Arc::new(Mutex::new(pair.master.take_writer().expect("stdin")));
    let mut reader = pair.master.try_clone_reader().expect("外部reader");
    let mut command = CommandBuilder::new("powershell.exe");
    command.args(["-NoLogo", "-NoProfile", "-Command", "[Console]::WriteLine('NORMA_CONPTY_DROP_READY'); while ($true) { [Console]::WriteLine(('x' * 1024)) }"]);
    let mut child = pair.slave.spawn_command(command).expect("真实写入进程");
    drop(pair.slave);
    let reply = writer.clone();
    let (ready, observed) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut recent = Vec::new();
        let mut bytes = [0u8; 4096];
        let mut replied = false;
        loop {
            let count = reader.read(&mut bytes).expect("启动输出");
            assert!(count > 0, "达到ready前不能EOF");
            recent.extend_from_slice(&bytes[..count]);
            if !replied && recent.windows(4).any(|p| p == b"\x1b[6n") {
                reply
                    .lock()
                    .expect("stdin锁")
                    .write_all(b"\x1b[1;1R")
                    .expect("实际DSR响应");
                replied = true;
            }
            if recent
                .windows(b"NORMA_CONPTY_DROP_READY".len())
                .any(|p| p == b"NORMA_CONPTY_DROP_READY")
            {
                drop(reader);
                ready.send(()).expect("报告外部reader已释放");
                return;
            }
            if recent.len() > 128 {
                recent.drain(..recent.len() - 128);
            }
        }
    });
    observed
        .recv_timeout(Duration::from_secs(10))
        .expect("必须实际完成启动并释放reader");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut available = 0;
    while Instant::now() < deadline {
        assert_ne!(
            unsafe {
                PeekNamedPipe(
                    raw as _,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut available,
                    std::ptr::null_mut(),
                )
            },
            0,
            "真实Peek不能失败"
        );
        if available >= capacity {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    let full = available >= capacity;
    let running = child.try_wait().expect("查询精确客户端句柄").is_none();
    eprintln!(
        "ConPTY close precondition: available={}, capacity={}, writer_running={}",
        available, capacity, running
    );
    let (closed, completed) = mpsc::sync_channel(1);
    let master = pair.master;
    thread::spawn(move || {
        drop(master);
        let _ = closed.send(());
    });
    let returned = completed.recv_timeout(Duration::from_secs(3)).is_ok();
    let _ = child.kill();
    let _ = child.wait();
    drop(writer);
    assert!(
        full,
        "必须真实达到管道容量：available={}, capacity={}",
        available, capacity
    );
    assert!(running, "销毁前写入者必须仍然存活，不能把提前退出算成功");
    assert!(
        returned,
        "外部reader失效且输出管道满时，master销毁必须在3秒内返回"
    );
}
