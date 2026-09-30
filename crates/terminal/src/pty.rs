//! portable-pty 封装：spawn 默认 shell、读线程桥接、resize、write、kill。
use crate::message::PtyEvent;
use crossbeam_channel::Sender;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, PtySize};
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PtyCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(String, String)>,
}

impl PtyCommand {
    pub fn new(program: impl Into<String>) -> Self {
        PtyCommand {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for arg in args {
            self.args.push(arg.into());
        }
        self
    }

    pub fn cwd(mut self, path: impl Into<PathBuf>) -> Self {
        self.cwd = Some(path.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    pub fn with_cwd(&self, path: Option<&Path>) -> Self {
        let mut command = self.clone();
        command.cwd = path.map(Path::to_path_buf);
        command
    }
}

pub trait PtyBackend {
    fn write(&mut self, bytes: &[u8]) -> Result<(), PtyError>;
    fn resize(&mut self, cols: u16, rows: u16) -> Result<(), PtyError>;
    fn kill(&mut self);
    fn child_pid(&self) -> Option<u32> {
        None
    }
    fn child_process_observer(&self) -> Option<crate::ChildProcessObserver> {
        None
    }
}

pub struct PtyHandle {
    master: Box<dyn portable_pty::MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn ChildKiller + Send + Sync>,
    child_process: crate::process_tree::ManagedProcess,
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
        Self::spawn_command(PtyCommand::new(shell), cols, rows, tx)
    }

    pub fn spawn_command(
        command: PtyCommand,
        cols: u16,
        rows: u16,
        tx: Sender<PtyEvent>,
    ) -> Result<PtyHandle, PtyError> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::Spawn(e.to_string()))?;

        let mut cmd = CommandBuilder::new(OsStr::new(&command.program));
        for arg in &command.args {
            cmd.arg(OsStr::new(arg));
        }
        if let Some(cwd) = &command.cwd {
            cmd.cwd(cwd);
        }
        for (key, value) in &command.env {
            cmd.env(OsStr::new(key), OsStr::new(value));
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| PtyError::Spawn(e.to_string()))?;
        let child_process = crate::process_tree::capture(child.process_id());
        let child_killer = child.clone_killer();
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

        // 读线程：阻塞读 → 发 Output；EOF → wait 子进程后发 Exited 并退出。
        thread::Builder::new()
            .name("pty-reader".into())
            .spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => {
                            let _ = tx.send(PtyEvent::Exited {
                                code: child_exit_code(child.wait()),
                            });
                            break;
                        }
                        Ok(n) => {
                            // 通道断开（app 已关闭）→ 退出读线程，避免僵尸。
                            if tx.send(PtyEvent::Output(buf[..n].to_vec())).is_err() {
                                let _ = child.kill();
                                let _ = child.wait();
                                break;
                            }
                        }
                        Err(_) => {
                            let _ = tx.send(PtyEvent::Exited {
                                code: child_exit_code(child.wait()),
                            });
                            break;
                        }
                    }
                }
            })
            .map_err(PtyError::Io)?;

        Ok(PtyHandle {
            master: pair.master,
            writer,
            child: child_killer,
            child_process,
        })
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        self.writer.write_all(bytes)?;
        self.writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), PtyError> {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::Resize(e.to_string()))
    }

    /// 杀掉子进程（窗口关闭时调用，确保不留僵尸进程）。
    pub fn kill(&mut self) {
        crate::process_tree::terminate_or_signal(self.child_process, self.child.as_mut());
    }
}

/// 默认实现：PtyHandle 即真实 PTY 后端，全限定委托固有方法，不改动其公开 API。
impl PtyBackend for PtyHandle {
    fn write(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        PtyHandle::write(self, bytes)
    }

    fn resize(&mut self, cols: u16, rows: u16) -> Result<(), PtyError> {
        PtyHandle::resize(self, cols, rows)
    }

    fn kill(&mut self) {
        PtyHandle::kill(self)
    }

    fn child_pid(&self) -> Option<u32> {
        self.child_process.pid()
    }
    fn child_process_observer(&self) -> Option<crate::ChildProcessObserver> {
        self.child_process.observer()
    }
}

fn child_exit_code(status: std::io::Result<portable_pty::ExitStatus>) -> Option<i32> {
    status
        .ok()
        .map(|status| status.exit_code().min(i32::MAX as u32) as i32)
}

impl Drop for PtyHandle {
    fn drop(&mut self) {
        // 任何持有路径释放时都确保子进程被回收，避免僵尸。
        self.kill();
    }
}

#[cfg(test)]
#[path = "../tests/unit/pty_tests.rs"]
mod tests;
