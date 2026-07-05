//! Terminal-backed LiveNode implementation.

use crate::live::{Caps, LiveNode, NodeError, NodeInput, NodeResize, NodeScroll, NodeView};
use terminal::{PtyCommand, PtyError, TerminalSession, TerminalSessionId};

pub struct TermLive {
    session: TerminalSession,
    cached_view: NodeView,
    #[cfg(test)]
    cached_view_refreshes: usize,
}

impl TermLive {
    pub fn spawn(
        id: TerminalSessionId,
        shell: &str,
        cols: u16,
        rows: u16,
    ) -> Result<Self, PtyError> {
        Self::from_session(TerminalSession::spawn(id, shell, cols, rows)?)
    }

    pub fn spawn_command(
        id: TerminalSessionId,
        command: PtyCommand,
        cols: u16,
        rows: u16,
    ) -> Result<Self, PtyError> {
        Self::from_session(TerminalSession::spawn_command(id, command, cols, rows)?)
    }

    fn from_session(session: TerminalSession) -> Result<Self, PtyError> {
        let cached_view = Self::view_from_session(&session);
        Ok(Self {
            session,
            cached_view,
            #[cfg(test)]
            cached_view_refreshes: 1,
        })
    }

    fn view_from_session(session: &TerminalSession) -> NodeView {
        NodeView::Term {
            snapshot: session.snapshot(),
            status: session.status,
        }
    }

    fn refresh_cached_view(&mut self) {
        self.cached_view = Self::view_from_session(&self.session);
        #[cfg(test)]
        {
            self.cached_view_refreshes += 1;
        }
    }

    #[cfg(test)]
    pub(crate) fn session(&self) -> &TerminalSession {
        &self.session
    }

    #[cfg(test)]
    pub(crate) fn cached_view_refresh_count(&self) -> usize {
        self.cached_view_refreshes
    }
}

impl LiveNode for TermLive {
    fn caps(&self) -> Caps {
        Caps::TERM
    }

    fn drain(&mut self) -> bool {
        let dirty = self.session.drain();
        if dirty {
            self.refresh_cached_view();
        }
        dirty
    }

    fn terminal_status(&self) -> Option<terminal::TerminalStatus> {
        Some(self.session.status)
    }

    fn resize(&mut self, resize: NodeResize) -> Result<(), NodeError> {
        match resize {
            NodeResize::TermGrid { cols, rows } => {
                self.session.resize(cols, rows);
                self.refresh_cached_view();
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
            NodeScroll::TermLines(lines) => {
                let changed = self.session.scroll_display(lines);
                if changed {
                    self.refresh_cached_view();
                }
                Ok(changed)
            }
        }
    }

    fn view(&self) -> Option<NodeView> {
        Some(self.cached_view.clone())
    }

    fn kill(&mut self) {
        self.session.kill();
    }

    fn child_pid(&self) -> Option<u32> {
        self.session.child_pid()
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

    /// TermLive view 应复用缓存，避免 animation-only repaint 每次重建 terminal snapshot。
    ///
    /// 数据构造（含关键数值推导）：
    ///   初始尺寸 80×24。
    ///   cached_view 初始刷新次数 = 1（spawn 后生成第一份可渲染快照）。
    ///   重复 view 3 次没有 drain/resize/scroll，因此刷新次数仍应为 1。
    ///   resize 到 100×30 是真实 terminal view 变化，应刷新一次：1 + 1 = 2。
    ///
    /// 执行过程：
    ///   1. spawn /bin/sh 创建 TermLive。
    ///   2. 读取 cached_view_refresh_count 作为初始计数。
    ///   3. 连续调用 view() 三次，模拟只有边框动画触发的重绘。
    ///   4. resize(TermGrid{100,30}) 后再次读取计数。
    ///
    /// 预期结果：
    ///   - 正断言：初始缓存刷新次数为 1，说明 spawn 后已有 cached view。
    ///   - 正断言：重复 view 后刷新次数仍为 1，证明 view 只 clone 缓存。
    ///   - 正断言：resize 后刷新次数为 2，证明 terminal 内容结构变化会刷新缓存。
    ///   - 负断言：重复 view 后刷新次数不应为 4，否则每次 animation repaint 都会重建 snapshot。
    #[test]
    fn test_term_live_view_reuses_cached_snapshot_until_terminal_changes() {
        let mut live = TermLive::spawn(4, "/bin/sh", 80, 24).expect("TermLive spawn 应成功");

        let initial_refresh_count = live.cached_view_refresh_count();
        let _first_view = live.view().expect("首次 view 应返回 cached terminal view");
        let _second_view = live
            .view()
            .expect("第二次 view 应返回 cached terminal view");
        let _third_view = live
            .view()
            .expect("第三次 view 应返回 cached terminal view");
        let repeated_view_refresh_count = live.cached_view_refresh_count();
        live.resize(NodeResize::TermGrid {
            cols: 100,
            rows: 30,
        })
        .expect("TermGrid resize 应成功");
        let resized_refresh_count = live.cached_view_refresh_count();

        assert_eq!(
            initial_refresh_count, 1,
            "spawn 后应生成 1 次 cached terminal view，作为初始渲染快照"
        );
        assert_eq!(
            repeated_view_refresh_count, 1,
            "连续 view() 不应刷新 snapshot；否则 animation-only repaint 仍会重建 terminal grid"
        );
        assert_eq!(
            resized_refresh_count, 2,
            "resize 改变 terminal view 尺寸后应刷新 cached snapshot"
        );
        assert_ne!(
            repeated_view_refresh_count, 4,
            "三次 view() 后刷新次数不应变成 4，否则缓存没有生效"
        );

        live.kill();
    }
}
