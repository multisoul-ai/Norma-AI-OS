//! TerminalSession：把 PtyHandle + TerminalGrid + 通道接收端组装为一个可 drain 的会话。

use crate::grid::{GridSnapshot, TerminalGrid};
use crate::input::{encode, KeyInput};
use crate::message::PtyEvent;
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

    /// 批量排空通道：把这一批 PTY 输出一次性喂给 grid（8–16ms 批处理由 app 定时调用实现）。
    /// 返回是否 dirty（需要 repaint）。
    pub fn drain(&mut self) -> bool {
        let mut dirty = false;
        loop {
            match self.rx.try_recv() {
                Ok(PtyEvent::Output(bytes)) => {
                    self.grid.advance(&bytes);
                    dirty = true;
                }
                Ok(PtyEvent::Exited { code }) => {
                    self.status = TerminalStatus::Exited { code };
                    dirty = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // 读线程已结束 → 标记退出（若尚未标记）。
                    if self.status == TerminalStatus::Running {
                        self.status = TerminalStatus::Exited { code: None };
                        dirty = true;
                    }
                    break;
                }
            }
        }
        dirty
    }

    pub fn write_input(&mut self, input: &KeyInput) -> Result<(), PtyError> {
        let bytes = encode(input);
        self.pty.write(&bytes)
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        self.pty.write(bytes)
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

    pub fn scroll_display(&mut self, lines: i32) -> bool {
        self.grid.scroll_display(lines)
    }

    pub fn kill(&mut self) {
        self.pty.kill();
    }

    pub fn child_pid(&self) -> Option<u32> {
        self.pty.child_pid()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Key;
    use std::time::{Duration, Instant};

    /// 集成：spawn sh，写入命令，drain 后快照含命令输出，exit 后 status=Exited。
    ///
    /// 数据构造：
    ///   终端尺寸：80 列 × 24 行
    ///   写入命令：b"printf ABC; exit\n"
    ///   子进程：/bin/sh（macOS 原生 shell）
    ///
    /// 执行过程：
    ///   1. spawn("/bin/sh", 80, 24) → 内部创建 crossbeam 通道，PtyHandle 启动读线程
    ///      session.status = Running
    ///   2. write_bytes(b"printf ABC; exit\n") → sh 执行 printf 输出 "ABC" 后调用 exit
    ///   3. 循环 drain（每 20ms 一次，最多 3 秒）：
    ///      - PTY 读线程把原始字节投递 Output(bytes) → drain 喂给 grid.advance → dirty=true
    ///      - sh 退出 → 读线程投递 Exited{code} → drain 更新 status → dirty=true
    ///   4. 检测 status==Exited 后提前退出循环
    ///   5. snapshot() → grid 当前屏幕快照
    ///
    /// 预期结果：
    ///   - 正断言：至少一次 drain 返回 dirty=true（有实际输出）
    ///   - 正断言：快照首行包含 'A' 'B' 'C'（printf 输出被 grid 解析并落盘）
    ///   - 正断言：最终 status == Exited（子进程退出已被感知）
    ///   - 负断言：status != Running（shell 已退出，不应仍为 Running）
    #[test]
    fn test_session_runs_command_and_marks_exit() {
        let mut s = TerminalSession::spawn(1, "/bin/sh", 80, 24).expect("spawn 应成功");
        s.write_bytes(b"printf ABC; exit\n")
            .expect("write_bytes 应成功");

        let mut any_dirty = false;
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if s.drain() {
                any_dirty = true;
            }
            if matches!(s.status, TerminalStatus::Exited { .. }) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        let snap = s.snapshot();
        let first_row: String = snap.rows[0].iter().map(|c| c.c).collect();

        assert!(
            any_dirty,
            "运行命令后至少一次 drain 应返回 dirty=true，否则说明通道未收到输出"
        );
        assert!(
            first_row.contains("ABC"),
            "首行应包含命令输出 'ABC'，实际: {:?}",
            first_row
        );
        assert!(
            matches!(s.status, TerminalStatus::Exited { .. }),
            "shell exit 后 session 状态应为 Exited，实际: {:?}",
            s.status
        );
        assert_ne!(
            s.status,
            TerminalStatus::Running,
            "shell 已退出，status 不应仍为 Running"
        );
    }

    /// spawn_command 将程序参数传给子进程，命令输出进入 grid 并退出。
    ///
    /// 数据构造（含关键数值推导）：
    ///   PtyCommand:
    ///     program = "/bin/sh"
    ///     args    = ["-lc", "printf ARG_OK; exit"]
    ///   终端尺寸：80 列 × 24 行
    ///   session_id = 5（固定值，验证构造路径不改写 id）
    ///
    /// 执行过程：
    ///   1. spawn_command(5, command, 80, 24) → CommandBuilder 以 /bin/sh 为 argv[0]
    ///   2. CommandBuilder 追加 "-lc" 和 "printf ARG_OK; exit"
    ///   3. 子进程执行 printf，PTY 读线程投递 Output(bytes)
    ///   4. 循环 drain 直到 status=Exited 或 3 秒超时
    ///   5. snapshot() 汇总屏幕文本
    ///
    /// 预期结果：
    ///   - 正断言：session.id == 5
    ///   - 正断言：屏幕文本包含 "ARG_OK"（证明参数命令实际执行）
    ///   - 正断言：最终 status == Exited
    ///   - 负断言：屏幕文本不应包含 "ARG_MISSING"（不是误读默认 shell 输出）
    #[test]
    fn test_spawn_command_passes_arguments_to_child_process() {
        let command = PtyCommand::new("/bin/sh").args(["-lc", "printf ARG_OK; exit"]);
        let mut s =
            TerminalSession::spawn_command(5, command, 80, 24).expect("spawn_command 应成功");

        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            s.drain();
            if matches!(s.status, TerminalStatus::Exited { .. }) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        let snap = s.snapshot();
        let screen_text: String = snap
            .rows
            .iter()
            .flat_map(|row| row.iter().map(|cell| cell.c))
            .collect();

        assert_eq!(s.id, 5, "spawn_command 应保留传入的 session_id=5");
        assert!(
            screen_text.contains("ARG_OK"),
            "带参数命令应输出 ARG_OK，实际屏幕文本: {:?}",
            screen_text
        );
        assert!(
            matches!(s.status, TerminalStatus::Exited { .. }),
            "命令包含 exit，drain 后 session 应进入 Exited，实际: {:?}",
            s.status
        );
        assert!(
            !screen_text.contains("ARG_MISSING"),
            "屏幕文本不应包含未执行命令的哨兵 ARG_MISSING，实际: {:?}",
            screen_text
        );
    }

    /// write_input 经 encode 写 PTY：Enter 不报错（编码路径打通的最小验证）。
    ///
    /// 数据构造：
    ///   KeyInput { key: Key::Enter, ctrl: false, alt: false }
    ///   encode(Enter) → [0x0d]（CR 字节）
    ///
    /// 执行过程：
    ///   1. spawn("/bin/sh", 80, 24) → 会话就绪
    ///   2. write_input(Enter) → encode 产出 [0x0d] → pty.write([0x0d]) → flush
    ///   3. kill() → 确保不留僵尸进程
    ///
    /// 预期结果：
    ///   - 正断言：write_input(Enter) 返回 Ok（写入路径全程无错误）
    ///   - 负断言：返回不是 Err（PTY 已启动，写入不应失败）
    #[test]
    fn test_write_input_encodes_without_error() {
        let mut s = TerminalSession::spawn(2, "/bin/sh", 80, 24).expect("spawn 应成功");
        let result = s.write_input(&KeyInput {
            key: Key::Enter,
            ctrl: false,
            alt: false,
        });
        assert!(result.is_ok(), "write_input(Enter) 应成功写入 PTY，返回 Ok");
        assert!(
            result.is_ok(),
            "write_input(Enter) 不应返回 Err，PTY 处于运行状态"
        );
        s.kill();
    }

    /// drain 在通道为空时返回 false（无数据 → 无需 repaint）。
    ///
    /// 数据构造：
    ///   spawn 后立刻 drain（shell 启动需要时间，通常通道尚空）
    ///   注意：shell 可能已经产出提示符，所以我们先 kill 再等待通道耗尽后测试
    ///
    /// 执行过程：
    ///   1. spawn("/bin/sh", 80, 24) → 通道可能已有初始输出
    ///   2. kill() → 让读线程尽快结束
    ///   3. 等待通道耗尽（最多 500ms drain 循环，直到 status=Exited）
    ///   4. 再次 drain() → 通道已空 + 已 Exited → 返回 false
    ///
    /// 预期结果：
    ///   - 正断言：耗尽后再次 drain 返回 false（无新数据）
    ///   - 正断言：status 为 Exited（kill 后通道断开被感知）
    #[test]
    fn test_drain_returns_false_when_channel_empty() {
        let mut s = TerminalSession::spawn(3, "/bin/sh", 80, 24).expect("spawn 应成功");
        s.kill();

        // 等待通道耗尽并感知到 Disconnected/Exited（最多 1 秒）
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            s.drain();
            if matches!(s.status, TerminalStatus::Exited { .. }) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        // 此时通道已空，status 已 Exited → drain 应返回 false
        let dirty = s.drain();
        assert!(
            !dirty,
            "通道耗尽且已 Exited 后，drain 应返回 false（无新数据）"
        );
        assert!(
            matches!(s.status, TerminalStatus::Exited { .. }),
            "kill 后 status 应为 Exited，实际: {:?}",
            s.status
        );
    }

    /// resize 同步更新 cols/rows 字段，不 panic。
    ///
    /// 数据构造：
    ///   初始尺寸 80×24，resize 到 120×40
    ///
    /// 执行过程：
    ///   1. spawn("/bin/sh", 80, 24)
    ///   2. resize(120, 40) → 更新 session.cols/rows + grid.resize + pty.resize
    ///   3. snapshot() → 确认快照尺寸已更新
    ///
    /// 预期结果：
    ///   - 正断言：session.cols == 120，session.rows == 40
    ///   - 正断言：snapshot().cols == 120，snapshot().lines == 40
    ///   - 负断言：session.cols != 80（旧值已被覆盖）
    #[test]
    fn test_resize_updates_dimensions() {
        let mut s = TerminalSession::spawn(4, "/bin/sh", 80, 24).expect("spawn 应成功");
        s.resize(120, 40);

        assert_eq!(s.cols, 120, "resize 后 session.cols 应为 120");
        assert_eq!(s.rows, 40, "resize 后 session.rows 应为 40");
        assert_ne!(s.cols, 80, "resize 后 cols 不应仍为旧值 80");

        let snap = s.snapshot();
        assert_eq!(snap.cols, 120, "resize 后快照列数应为 120");
        assert_eq!(snap.lines, 40, "resize 后快照行数应为 40");

        s.kill();
    }

    /// session id 字段被正确存储。
    ///
    /// 数据构造：
    ///   用 id=42 创建 session
    ///
    /// 预期结果：
    ///   - 正断言：session.id == 42
    ///   - 负断言：session.id != 0（不是零值）
    #[test]
    fn test_session_id_stored() {
        let mut s = TerminalSession::spawn(42, "/bin/sh", 80, 24).expect("spawn 应成功");
        assert_eq!(s.id, 42, "session.id 应等于传入的 id=42");
        assert_ne!(s.id, 0, "session.id 不应为零值");
        s.kill();
    }

    /// 注入式 e2e：spawn_with_backend + TestPtyBackend::echo(tx) 驱动 write→drain→snapshot，
    /// 验证 backend 注入路径闭环且不启动真实子进程。
    ///
    /// 数据构造（含关键数值推导）：
    ///   通道：unbounded() → (tx, rx) 一对
    ///   backend = TestPtyBackend::echo(tx)：write(bytes) 经 tx.send(Output(bytes)) 同步回吐
    ///   session = spawn_with_backend(7, 80, 24, Box::new(backend), rx)
    ///     —— backend 持 tx（write 走 tx），session 持 rx（drain 走 rx），共享通道闭环
    ///   写入：write_bytes(b"hi") = 2 字节
    ///     echo 模式 → emit(b"hi") → tx.send(Output(b"hi")) 同步成功 → rx 立即可取
    ///
    /// 执行过程：
    ///   1. 建通道 (tx, rx)
    ///   2. echo(tx) 构造 backend，tx 所有权移入 backend
    ///   3. spawn_with_backend(7, 80, 24, Box::new(backend), rx) 注入 session，rx 移入 session
    ///   4. write_bytes(b"hi") → backend.write → emit → tx.send(Output) → 通道收到事件
    ///   5. drain() 从 rx 取 Output(b"hi") → grid.advance → dirty=true，应返回 true
    ///   6. 再次 drain() 通道已空 → 返回 false
    ///   7. snapshot() 取 grid 当前屏幕
    ///
    /// 预期结果：
    ///   - 正断言：首次 drain 返回 true（注入后端确实回吐了事件到通道）
    ///   - 正断言：快照首行包含 "hi"（echo 回吐的字节经 grid.advance 落盘）
    ///   - 正断言：session.id == 7（注入路径保留 id）
    ///   - 负断言：第二次 drain 返回 false（通道无遗留，无幽灵事件）
    ///   - 负断言：快照首行不应包含未写入的 "bye"
    #[test]
    fn test_spawn_with_backend_echo_drives_session() {
        use crate::test_backend::TestPtyBackend;

        let (tx, rx) = unbounded();
        let backend = TestPtyBackend::echo(tx);
        let mut s = TerminalSession::spawn_with_backend(7, 80, 24, Box::new(backend), rx);

        s.write_bytes(b"hi").expect("write_bytes 经注入后端应成功");

        let dirty_first = s.drain();
        let dirty_second = s.drain();

        let snap = s.snapshot();
        let first_row: String = snap.rows[0].iter().map(|c| c.c).collect();

        assert!(
            dirty_first,
            "注入后端 write 后应首次 drain 返回 true，否则说明通道未收到 echo 回吐"
        );
        assert!(
            first_row.contains("hi"),
            "快照首行应含 echo 回吐的 'hi'，实际: {:?}",
            first_row
        );
        assert_eq!(s.id, 7, "注入构造应保留 session_id=7");
        assert!(
            !dirty_second,
            "通道应已耗尽，第二次 drain 不应再返回 true（无幽灵事件）"
        );
        assert!(
            !first_row.contains("bye"),
            "快照首行不应含未写入的 'bye'，实际: {:?}",
            first_row
        );
    }

    /// T-PID-1: spawn_command 成功后 child_pid 应为 Some(非零值)。
    ///
    /// 数据构造：
    ///   command = "sleep 100"（保证进程存活足够长，避免 PID 在读取前消失）
    ///   cols = 80, rows = 24（标准终端尺寸）
    ///   session_id = 1
    ///
    /// 执行过程：
    ///   1. spawn_command(id=1, PtyCommand::new("sleep").args(["100"]), 80, 24)
    ///      → PtyHandle::spawn_command 在 clone_killer() 前调用 child.process_id()
    ///      → child_pid 被捕获并存入 PtyHandle.child_pid
    ///   2. session.child_pid() 委托给 self.pty.child_pid 返回 Option<u32>
    ///
    /// 预期结果：
    ///   - 正断言：child_pid 是 Some(pid)，spawn 成功后 PID 必须存在
    ///   - 正断言：pid > 0，OS 分配的进程 ID 始终大于 0
    #[test]
    fn test_spawn_sets_child_pid() {
        let mut session =
            TerminalSession::spawn_command(1, PtyCommand::new("sleep").args(["100"]), 80, 24)
                .unwrap();

        let pid = session.child_pid();
        assert!(
            pid.is_some(),
            "spawn 后 child_pid 应为 Some，实际为 None — process_id() 未在 clone_killer() 前调用"
        );
        assert!(
            pid.unwrap() > 0,
            "PID 应大于 0，实际为 {} — OS 不应分配 PID 0 给普通进程",
            pid.unwrap()
        );

        session.kill();
    }
}
