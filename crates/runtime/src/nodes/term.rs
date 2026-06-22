//! Terminal-backed LiveNode implementation.

use crate::live::{Caps, LiveNode, NodeError, NodeInput, NodeResize, NodeScroll, NodeView};
use terminal::{PtyCommand, PtyError, TerminalSession, TerminalSessionId};

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

    #[cfg(test)]
    pub(crate) fn session(&self) -> &TerminalSession {
        &self.session
    }
}

impl LiveNode for TermLive {
    fn caps(&self) -> Caps {
        Caps::TERM
    }

    fn drain(&mut self) -> bool {
        self.session.drain()
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
}
