//! TerminalSession：把 PtyHandle + TerminalGrid + 通道接收端组装为一个可 drain 的会话。

use crate::grid::{terminal_wheel_action, Cell, GridSnapshot, TerminalGrid, TerminalWheelAction};
use crate::input::{encode_for_keyboard_mode, Key, KeyInput};
use crate::message::PtyEvent;
use crate::paste::encode_paste;
use crate::pty::{PtyBackend, PtyCommand, PtyError, PtyHandle};
use crossbeam_channel::{unbounded, Receiver, TryRecvError};

pub type TerminalSessionId = u64;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalStatus {
    Running,
    Exited { code: Option<i32> },
}

pub struct TerminalSession {
    pub id: TerminalSessionId,
    pub status: TerminalStatus,
    pub cols: u16,
    pub rows: u16,
    pty: Box<dyn PtyBackend>,
    grid: TerminalGrid,
    rx: Receiver<PtyEvent>,
}

impl TerminalSession {
    pub fn spawn(
        id: TerminalSessionId,
        shell: &str,
        cols: u16,
        rows: u16,
    ) -> Result<TerminalSession, PtyError> {
        Self::spawn_command(id, PtyCommand::new(shell), cols, rows)
    }

    pub fn spawn_command(
        id: TerminalSessionId,
        command: PtyCommand,
        cols: u16,
        rows: u16,
    ) -> Result<TerminalSession, PtyError> {
        let (tx, rx) = unbounded();
        let pty: Box<dyn PtyBackend> = Box::new(PtyHandle::spawn_command(command, cols, rows, tx)?);
        let grid = TerminalGrid::new(cols, rows);
        Ok(TerminalSession {
            id,
            status: TerminalStatus::Running,
            cols,
            rows,
            pty,
            grid,
            rx,
        })
    }

    /// 注入式构造：不启动真实子进程。调用方预先建立 `(tx, rx)` 通道对：
    /// 把 `tx` 喂给 backend（如 `TestPtyBackend::echo(tx)`），backend 在
    /// `write` 时通过 `tx` 同步回吐 `PtyEvent::Output`；`rx` 交给 session 持有，
    /// `drain` 从 `rx` 取事件喂 grid。`backend` 与 `rx` 共享同一通道，闭环成立。
    pub fn spawn_with_backend(
        id: TerminalSessionId,
        cols: u16,
        rows: u16,
        backend: Box<dyn PtyBackend>,
        rx: Receiver<PtyEvent>,
    ) -> TerminalSession {
        let grid = TerminalGrid::new(cols, rows);
        TerminalSession {
            id,
            status: TerminalStatus::Running,
            cols,
            rows,
            pty: backend,
            grid,
            rx,
        }
    }

    /// 批量排空通道：把这一批 PTY 输出一次性喂给 grid（8–16ms 批处理由 app 定时调用实现），返回是否 dirty。
    pub fn drain(&mut self) -> bool {
        self.drain_at(std::time::Instant::now())
    }

    fn drain_at(&mut self, now: std::time::Instant) -> bool {
        let mut dirty = false;
        loop {
            match self.rx.try_recv() {
                Ok(PtyEvent::Output(bytes)) => {
                    let replies = self.grid.advance(&bytes);
                    if !replies.is_empty() {
                        let _ = self.write_pty(&replies);
                    }
                    dirty = true;
                }
                Ok(PtyEvent::Exited { code }) => {
                    let _ = self.grid.finish_output();
                    self.status = TerminalStatus::Exited { code };
                    dirty = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if self.grid.finish_output().is_some() {
                        dirty = true;
                    }
                    // 读线程已结束 → 标记退出（若尚未标记）。
                    if self.status == TerminalStatus::Running {
                        self.status = TerminalStatus::Exited { code: None };
                        dirty = true;
                    }
                    break;
                }
            }
        }
        if let Some(replies) = self.grid.expire_synchronized_output(now) {
            if !replies.is_empty() {
                let _ = self.write_pty(&replies);
            }
            dirty = true;
        }
        dirty
    }

    pub fn write_input(&mut self, input: &KeyInput) -> Result<(), PtyError> {
        let enhanced_keyboard = self.grid.enhanced_keyboard_enabled();
        // Claude 等 TUI 可能只声明 bracketed paste；用粘贴换行保留编辑语义，
        // 不把 Shift+Enter 降级为提交。已协商 Kitty 时仍优先发送原始按键。
        if !enhanced_keyboard
            && self.grid.bracketed_paste_enabled()
            && input.key == Key::Enter
            && input.shift
            && !input.ctrl
            && !input.alt
        {
            return self.write_paste("\n");
        }
        let bytes = encode_for_keyboard_mode(input, enhanced_keyboard);
        self.write_pty(&bytes)
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        self.write_pty(bytes)
    }

    /// 按当前终端应用声明的 paste mode 写入一整段剪贴板文本。
    ///
    /// bracketed paste 开启时，逻辑换行统一为 LF 并放入 begin/end 标记之间，
    /// 使 TUI 将整段内容作为一次 paste 处理；普通模式继续使用终端显示所需的 CRLF。
    pub fn write_paste(&mut self, text: &str) -> Result<(), PtyError> {
        let bytes = encode_paste(text, self.grid.bracketed_paste_enabled());
        self.write_pty(&bytes)
    }

    fn write_pty(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        if self.status != TerminalStatus::Running {
            return Err(PtyError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "terminal session has exited",
            )));
        }
        self.pty.write(bytes)
    }

    pub fn bracketed_paste_enabled(&self) -> bool {
        self.grid.bracketed_paste_enabled()
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.rows = rows;
        self.grid.resize(cols, rows);
        let _ = self.pty.resize(cols, rows);
    }

    pub fn snapshot(&self) -> GridSnapshot {
        self.grid.snapshot()
    }

    /// 读取绝对行范围 `[start_abs, end_abs]`（含两端）的中性 Cell 行数据。
    /// 绝对行坐标系见 [`TerminalGrid::rows_for_absolute`]。
    pub fn rows_for_absolute(&self, start_abs: usize, end_abs: usize) -> Vec<Vec<Cell>> {
        self.grid.rows_for_absolute(start_abs, end_abs)
    }

    pub fn scroll_display(&mut self, lines: i32) -> bool {
        self.grid.scroll_display(lines)
    }

    pub fn scroll_wheel(&mut self, lines: i32, row: u16, col: u16) -> Result<bool, PtyError> {
        match terminal_wheel_action(&self.grid.snapshot(), lines, row, col) {
            TerminalWheelAction::Scrollback(lines) => Ok(self.grid.scroll_display(lines)),
            TerminalWheelAction::WriteBytes(bytes) => {
                self.write_pty(&bytes)?;
                Ok(false)
            }
        }
    }

    pub fn kill(&mut self) {
        self.pty.kill();
        if self.status == TerminalStatus::Running {
            self.status = TerminalStatus::Exited { code: None };
        }
    }

    pub fn child_pid(&self) -> Option<u32> {
        self.pty.child_pid()
    }
    pub fn child_process_observer(&self) -> Option<crate::ChildProcessObserver> {
        self.pty.child_process_observer()
    }
}

#[cfg(test)]
#[path = "../tests/unit/session_backend_tests.rs"]
mod backend_tests;

#[cfg(test)]
#[path = "../tests/unit/session_lifecycle_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/unit/session_sync_tests.rs"]
mod sync_tests;
