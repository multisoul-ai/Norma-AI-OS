//! Terminal-backed LiveNode implementation.

use crate::live::{Caps, LiveNode, NodeError, NodeInput, NodeResize, NodeScroll, NodeView};
use crossbeam_channel::{unbounded, Sender};
use terminal::{PtyBackend, PtyCommand, PtyError, PtyEvent, TerminalSession, TerminalSessionId};

/// PTY 后端工厂：给定 (命令, cols, rows, tx) 返回 `Box<dyn PtyBackend>`。
///
/// **仅测试注入路径使用**。生产路径保留 `TermLive::spawn_command` →
/// `PtyHandle::spawn_command`（PtyHandle 内部自建 tx/rx 通道喂读线程）。真实
/// `PtyHandle::spawn_command` 自建通道，无法套用本 signature（factory 需接收外部
/// tx），故生产 PTY 链路零改动。
///
/// 测试用法：factory 接收 `tx` 构建 `TestPtyBackend`（如
/// `Box::new(|_cmd, _c, _r, tx| Ok(Box::new(TestPtyBackend::echo(tx)) as Box<dyn PtyBackend>))`），
/// backend 在 `write` 时经 `tx` 同步回吐 `PtyEvent::Output`；配
/// `TerminalSession::spawn_with_backend` 形成 tx→backend、rx→session 闭环。
pub type PtyFactory = Box<
    dyn Fn(PtyCommand, u16, u16, Sender<PtyEvent>) -> Result<Box<dyn PtyBackend>, PtyError>
        + Send
        + Sync,
>;

pub struct TermLive {
    session: TerminalSession,
}

impl TermLive {
    pub fn spawn(
        id: TerminalSessionId,
        shell: &str,
        cols: u16,
        rows: u16,
    ) -> Result<Self, PtyError> {
        Ok(Self {
            session: TerminalSession::spawn(id, shell, cols, rows)?,
        })
    }

    pub fn spawn_command(
        id: TerminalSessionId,
        command: PtyCommand,
        cols: u16,
        rows: u16,
    ) -> Result<Self, PtyError> {
        Ok(Self {
            session: TerminalSession::spawn_command(id, command, cols, rows)?,
        })
    }

    /// 测试注入路径：用外部 `factory` 构建 PTY backend，配
    /// [`TerminalSession::spawn_with_backend`]。**仅测试用**——生产路径调用
    /// [`TermLive::spawn_command`]（`PtyHandle` 自建通道）。
    ///
    /// 通道配对：内部 `unbounded()` 建 `(tx, rx)`，`tx` 喂给 `factory` 构 backend，
    /// `rx` 交 session 持有；backend `write` 经 `tx` 同步回吐 `PtyEvent::Output`，
    /// session `drain` 从 `rx` 取事件喂 grid，闭环成立。
    pub fn spawn_with_factory(
        id: TerminalSessionId,
        command: PtyCommand,
        cols: u16,
        rows: u16,
        factory: &PtyFactory,
    ) -> Result<Self, PtyError> {
        let (tx, rx) = unbounded();
        let backend = factory(command, cols, rows, tx)?;
        Ok(Self {
            session: TerminalSession::spawn_with_backend(id, cols, rows, backend, rx),
        })
    }

    #[cfg(test)]
    pub(crate) fn session(&self) -> &TerminalSession {
        &self.session
    }
}

/// 工厂分发：`pty_factory` 为 `Some` 时走注入路径（测试），否则走生产
/// `TermLive::spawn_command`（真 PTY）。供 `Runtime` 的 spawn/restore 路径统一调用，
/// 避免在每个调用点重复 if/else。
pub(crate) fn spawn_term_live(
    pty_factory: Option<&PtyFactory>,
    id: TerminalSessionId,
    command: PtyCommand,
    cols: u16,
    rows: u16,
) -> Result<TermLive, PtyError> {
    match pty_factory {
        Some(factory) => TermLive::spawn_with_factory(id, command, cols, rows, factory),
        None => TermLive::spawn_command(id, command, cols, rows),
    }
}

impl LiveNode for TermLive {
    fn caps(&self) -> Caps {
        Caps::TERM
    }

    fn drain(&mut self) -> bool {
        self.session.drain()
    }

    fn terminal_status(&self) -> Option<terminal::TerminalStatus> {
        Some(self.session.status)
    }

    fn resize(&mut self, resize: NodeResize) -> Result<(), NodeError> {
        match resize {
            NodeResize::TermGrid { cols, rows } => {
                self.session.resize(cols, rows);
                Ok(())
            }
        }
    }

    fn input(&mut self, input: NodeInput) -> Result<(), NodeError> {
        match input {
            NodeInput::Key(key) => self.session.write_input(&key).map_err(NodeError::from),
            NodeInput::Text(s) => self
                .session
                .write_bytes(s.as_bytes())
                .map_err(NodeError::from),
        }
    }

    fn scroll(&mut self, scroll: NodeScroll) -> Result<bool, NodeError> {
        match scroll {
            NodeScroll::TermLines(lines) => Ok(self.session.scroll_display(lines)),
        }
    }

    fn view(&self) -> Option<NodeView> {
        Some(NodeView::Term {
            snapshot: self.session.snapshot(),
            status: self.session.status,
        })
    }

    fn kill(&mut self) {
        self.session.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TermLive resize 将 terminal grid 尺寸传递给内部 TerminalSession。
    ///
    /// 数据构造（含关键数值推导）：
    ///   初始尺寸 80×24
    ///   resize 目标尺寸 100×30
    ///   session id = 3（固定值，只用于构造，不参与尺寸计算）
    ///
    /// 执行过程：
    ///   1. spawn /bin/sh 创建 TermLive，内部 session.cols=80, rows=24
    ///   2. resize(TermGrid{100,30}) → session.resize(100,30)
    ///   3. 读取内部 session 尺寸验证
    ///   4. kill 清理子进程
    ///
    /// 预期结果：
    ///   - 正断言：resize 后 cols=100, rows=30
    ///   - 负断言：cols 不应仍为 80，rows 不应仍为 24
    #[test]
    fn test_term_live_resize_updates_terminal_session_dimensions() {
        let mut live = TermLive::spawn(3, "/bin/sh", 80, 24).expect("TermLive spawn 应成功");

        live.resize(NodeResize::TermGrid {
            cols: 100,
            rows: 30,
        })
        .expect("TermGrid resize 应成功");

        assert_eq!(
            live.session().cols,
            100,
            "resize 后 TerminalSession.cols 应更新为 100"
        );
        assert_eq!(
            live.session().rows,
            30,
            "resize 后 TerminalSession.rows 应更新为 30"
        );
        assert_ne!(
            live.session().cols,
            80,
            "resize 后 cols 不应仍停留在初始值 80"
        );
        assert_ne!(
            live.session().rows,
            24,
            "resize 后 rows 不应仍停留在初始值 24"
        );

        live.kill();
    }

    /// Runtime PTY 工厂注入全链路：set_pty_factory(echo) → spawn_terminal → write "hi"
    /// → drain_all → node_view 的 GridSnapshot 含 "hi"。验证生产 `spawn_command` 路径
    /// 被测试工厂替换为 `spawn_with_backend`，且 channel 闭环（factory 接 tx 构 backend、
    /// session 持 rx）成立：write 同步经 tx 回吐 Output，drain 从 rx 取事件喂 grid。
    ///
    /// 数据构造（含关键数值推导）：
    ///   echo 工厂 = `|_cmd, _cols, _rows, tx| Ok(Box::new(TestPtyBackend::echo(tx)))`
    ///     → backend.write(bytes) 原样 tx.send(Output(bytes)) 同步回吐
    ///   cols=80, rows=24（与 runtime DEFAULT_SESSION_COLS/ROWS 一致）
    ///   发送文本 "hi" = 2 字节 → echo 回吐 Output(b"hi") = 2 字节
    ///   grid.advance(b"hi") → 'h'@(line0,col0), 'i'@(line0,col1), cursor@(line0,col2)
    ///
    /// 执行过程：
    ///   1. Runtime::new() + set_pty_factory(echo)：pty_factory = Some(...)
    ///   2. spawn_terminal → spawn_term_live(Some) → TermLive::spawn_with_factory：
    ///      内部 unbounded() 建 (tx,rx)；tx 喂 echo_factory 构 TestPtyBackend；rx 入 session
    ///   3. spawn_terminal(bounds,"/bin/zsh",80,24) → node_id 注册，live_len=1
    ///   4. send_node_input(node_id, Text("hi")) → session.write_bytes(b"hi")
    ///      → pty.write(b"hi") → echo backend tx.send(Output(b"hi"))（同步，rx 立即可取）
    ///   5. drain_all() → session.drain() → rx.try_recv 取 Output(b"hi") → grid.advance → dirty
    ///   6. node_view(node_id) → NodeView::Term { snapshot, .. }；拼接 rows[0] 各 Cell.c
    ///
    /// 预期结果：
    ///   - 正断言：live_len == 1（恰好 1 个注入节点）
    ///   - 正断言：drain_all 返回 dirty（echo 回吐触发了 grid 更新）
    ///   - 正断言：node_view 为 NodeView::Term（而非 None）
    ///   - 正断言：首行文本以 "hi" 起始（echo 回吐内容落到 grid 首行）
    ///   - 负断言：首行文本不含未发送的 "bye"
    #[test]
    fn test_runtime_injected_pty_factory_echoes_input_into_grid() {
        use crate::{NodeInput, NodeView, Runtime};
        use canvas::geometry::CanvasRect;
        use terminal::TestPtyBackend;

        let echo_factory: PtyFactory = Box::new(
            |_cmd: PtyCommand, _cols: u16, _rows: u16, tx: Sender<PtyEvent>| {
                Ok(Box::new(TestPtyBackend::echo(tx)) as Box<dyn PtyBackend>)
            },
        );

        let mut runtime = Runtime::new();
        runtime.set_pty_factory(echo_factory);

        let bounds = CanvasRect::new(0.0, 0.0, 640.0, 400.0);
        let node_id = runtime
            .spawn_terminal(bounds, "/bin/zsh", 80, 24)
            .expect("spawn_terminal 走注入工厂应成功（无真实子进程）");
        assert_eq!(
            runtime.live_len(),
            1,
            "注入 spawn_terminal 后应注册恰好 1 个 live 节点"
        );

        runtime
            .send_node_input(node_id, NodeInput::Text("hi".to_string()))
            .expect("send_node_input(Text(\"hi\")) 应成功写入 echo backend");

        let dirty = runtime.drain_all();
        assert!(dirty, "drain_all 应因 echo 回吐的 Output 标记 dirty");

        // NodeView 当前仅 Term 单 variant，直接 irrefutable 解构取 snapshot。
        let NodeView::Term { snapshot, .. } = runtime
            .node_view(node_id)
            .expect("node_view 应返回 Some（节点存在）");

        let first_row: String = snapshot
            .rows
            .first()
            .expect("snapshot 应至少含 1 行")
            .iter()
            .map(|cell| cell.c)
            .collect();
        assert!(
            first_row.starts_with("hi"),
            "首行应以 echo 回吐的 'hi' 起始，实际: {:?}",
            first_row
        );
        assert!(
            !first_row.contains("bye"),
            "首行不应含未发送的 'bye'，实际: {:?}",
            first_row
        );

        runtime.kill_all();
    }
}
