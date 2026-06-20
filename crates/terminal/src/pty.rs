//! portable-pty 封装：spawn 默认 shell、读线程桥接、resize、write、kill。
//! 不引 tokio —— 读线程是普通 std::thread，数据经 crossbeam 通道回 app。

use crate::message::PtyEvent;
use crossbeam_channel::Sender;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::thread;

#[derive(Debug, thiserror::Error)]
pub enum PtyError {
    #[error("pty spawn failed: {0}")]
    Spawn(String),
    #[error("pty io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("pty resize failed: {0}")]
    Resize(String),
}

pub struct PtyHandle {
    master: Box<dyn portable_pty::MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

impl PtyHandle {
    /// 启动 shell（或指定命令）。cols/rows 为初始终端尺寸。
    /// 起一个读线程，把 PTY 输出经 tx 投递；进程退出时投递 Exited。
    pub fn spawn(
        shell: &str,
        cols: u16,
        rows: u16,
        tx: Sender<PtyEvent>,
    ) -> Result<PtyHandle, PtyError> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| PtyError::Spawn(e.to_string()))?;

        let cmd = CommandBuilder::new(shell);
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| PtyError::Spawn(e.to_string()))?;
        // slave 在 spawn 后即可释放（子进程已持有其 fd）。
        drop(pair.slave);

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| PtyError::Spawn(e.to_string()))?;
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| PtyError::Spawn(e.to_string()))?;

        // 读线程：阻塞读 → 发 Output；EOF → 发 Exited 并退出。
        thread::Builder::new()
            .name("pty-reader".into())
            .spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => {
                            let _ = tx.send(PtyEvent::Exited { code: None });
                            break;
                        }
                        Ok(n) => {
                            // 通道断开（app 已关闭）→ 退出读线程，避免僵尸。
                            if tx.send(PtyEvent::Output(buf[..n].to_vec())).is_err() {
                                break;
                            }
                        }
                        Err(_) => {
                            let _ = tx.send(PtyEvent::Exited { code: None });
                            break;
                        }
                    }
                }
            })
            .map_err(PtyError::Io)?;

        Ok(PtyHandle { master: pair.master, writer, child })
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        self.writer.write_all(bytes)?;
        self.writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), PtyError> {
        self.master
            .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| PtyError::Resize(e.to_string()))
    }

    /// 杀掉子进程（窗口关闭时调用，确保不留僵尸进程）。
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;
    use std::time::Duration;

    /// 冒烟集成测试：spawn `/bin/sh`，写入命令 `printf 'hello-pty'; exit`，
    /// 从通道收到含该文本的输出，最终收到 Exited。
    ///
    /// 说明：这是真实进程 + 真实 PTY 的集成测试（非纯逻辑单测），在本机 macOS 有 TTY 环境下稳定。
    ///
    /// 数据构造：
    ///   命令通过 stdin 写入 sh：`printf 'hello-pty'; exit\n`
    ///   输出预期包含 "hello-pty"（printf 不加换行，PTY 原样输出）
    ///
    /// 执行过程：
    ///   1. spawn /bin/sh → 读线程启动
    ///   2. write `printf 'hello-pty'; exit\n` → sh 执行命令
    ///   3. printf 输出 "hello-pty" → Output 事件入通道
    ///   4. exit → sh 退出 → 读线程读到 EOF → Exited 入通道
    ///
    /// 预期结果：
    ///   - 正断言：累积输出字节中包含子串 "hello-pty"
    ///   - 正断言：在 3 秒内收到至少一个 Exited 事件
    #[test]
    fn test_spawn_echo_emits_output_and_exit() {
        let (tx, rx) = unbounded();
        let mut pty = PtyHandle::spawn("/bin/sh", 80, 24, tx).expect("spawn sh");
        pty.write(b"printf 'hello-pty'; exit\n").expect("write cmd");

        let mut acc = Vec::new();
        let mut saw_exit = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(PtyEvent::Output(b)) => acc.extend_from_slice(&b),
                Ok(PtyEvent::Exited { .. }) => {
                    saw_exit = true;
                    break;
                }
                Err(_) => {}
            }
        }
        let text = String::from_utf8_lossy(&acc);
        assert!(
            text.contains("hello-pty"),
            "PTY 输出应包含 'hello-pty'，实际收到: {:?}",
            text
        );
        assert!(
            !text.contains("command not found"),
            "PTY 输出不应含错误信息，若有说明命令执行失败: {:?}",
            text
        );
        assert!(saw_exit, "shell exit 后应在超时内收到 Exited 事件");
    }
}
