//! 节点生命周期 + 终端会话注册表。把 canvas 文档与 terminal 会话组装在一起。

use crate::focus::FocusState;
use canvas::{CanvasRect, CanvasNodeKind, Document, NodeId, Selection, TerminalNodeRef};
use std::collections::HashMap;
use terminal::{PtyError, TerminalSession, TerminalSessionId};

pub struct Runtime {
    pub document: Document,
    pub selection: Selection,
    pub focus: FocusState,
    sessions: HashMap<TerminalSessionId, TerminalSession>,
    next_session_id: TerminalSessionId,
}

impl Default for Runtime {
    fn default() -> Self {
        Runtime::new()
    }
}

impl Runtime {
    pub fn new() -> Self {
        Runtime {
            document: Document::new(),
            selection: Selection::new(),
            focus: FocusState::new(),
            sessions: HashMap::new(),
            next_session_id: 1,
        }
    }

    /// 新建一个 Terminal 节点：spawn 会话 + 在画布加 Terminal 节点。返回画布节点 id。
    pub fn spawn_terminal(
        &mut self,
        bounds: CanvasRect,
        shell: &str,
        cols: u16,
        rows: u16,
    ) -> Result<NodeId, PtyError> {
        let sid = self.next_session_id;
        self.next_session_id += 1;
        let session = TerminalSession::spawn(sid, shell, cols, rows)?;
        self.sessions.insert(sid, session);
        let node_id = self
            .document
            .add_node(CanvasNodeKind::Terminal(TerminalNodeRef { session_id: sid }), bounds);
        Ok(node_id)
    }

    pub fn session_mut(&mut self, id: TerminalSessionId) -> Option<&mut TerminalSession> {
        self.sessions.get_mut(&id)
    }

    pub fn session(&self, id: TerminalSessionId) -> Option<&TerminalSession> {
        self.sessions.get(&id)
    }

    /// drain 所有会话；任一 dirty 则返回 true（app 据此决定是否 repaint）。
    pub fn drain_all(&mut self) -> bool {
        let mut dirty = false;
        for s in self.sessions.values_mut() {
            if s.drain() {
                dirty = true;
            }
        }
        dirty
    }

    /// 窗口关闭时杀掉所有子进程（SPEC §8）。
    pub fn kill_all(&mut self) {
        for s in self.sessions.values_mut() {
            s.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use canvas::geometry::CanvasRect;

    /// Runtime::new() 初始状态：document/selection/focus 均处于空/默认状态。
    ///
    /// 数据构造：
    ///   Runtime::new() — 不加入任何节点或会话
    ///
    /// 执行过程：
    ///   1. Runtime::new() → 各字段初始化
    ///   2. 检查 document 节点列表为空
    ///   3. 检查 focus 模式为 CanvasSelect
    ///
    /// 预期结果：
    ///   - 正断言：document.nodes() 为空
    ///   - 正断言：focus.mode() == CanvasSelect
    ///   - 负断言：document.nodes().len() != 1（未添加节点）
    #[test]
    fn test_runtime_new_is_empty() {
        use crate::focus::InputMode;
        let rt = Runtime::new();
        assert_eq!(rt.document.nodes().len(), 0, "新建 Runtime 文档应无节点");
        assert_ne!(rt.document.nodes().len(), 1, "未添加节点，文档不应有 1 个节点");
        assert_eq!(rt.focus.mode(), InputMode::CanvasSelect, "新建 Runtime 焦点应为 CanvasSelect");
    }

    /// spawn_terminal 同时注册会话并在 document 中创建 Terminal 节点。
    ///
    /// 数据构造：
    ///   bounds = CanvasRect::new(0, 0, 800, 600)
    ///   shell = "/bin/sh"，cols=80，rows=24
    ///   session_id（内部分配）= 1（next_session_id 初始为 1）
    ///
    /// 执行过程：
    ///   1. spawn_terminal(bounds, "/bin/sh", 80, 24)
    ///      → TerminalSession::spawn(1, "/bin/sh", 80, 24)
    ///      → sessions.insert(1, session)
    ///      → document.add_node(Terminal(TerminalNodeRef{session_id:1}), bounds) → node_id=1
    ///   2. 通过 session(1) 确认会话存在
    ///   3. 通过 document.node(node_id) 确认节点已创建
    ///
    /// 预期结果：
    ///   - 正断言：spawn_terminal 返回 Ok(node_id)
    ///   - 正断言：session(1) 返回 Some（会话已注册）
    ///   - 正断言：document.nodes().len() == 1（恰好一个节点）
    ///   - 负断言：session(99) 返回 None（不存在的会话）
    #[test]
    fn test_spawn_terminal_registers_session_and_node() {
        let mut rt = Runtime::new();
        let bounds = CanvasRect::new(0.0, 0.0, 800.0, 600.0);
        let result = rt.spawn_terminal(bounds, "/bin/sh", 80, 24);
        assert!(result.is_ok(), "spawn_terminal 应成功，返回 Ok(node_id)");

        // 内部 session_id 从 1 开始分配
        assert!(rt.session(1).is_some(), "session_id=1 应已注册到 sessions map");
        assert!(rt.session(99).is_none(), "session_id=99 未创建，应返回 None");

        assert_eq!(rt.document.nodes().len(), 1, "spawn 后 document 应恰好有 1 个节点");

        rt.kill_all();
    }

    /// kill_all 后不 panic；drain_all 在无会话时返回 false。
    ///
    /// 数据构造：
    ///   空 Runtime（无任何会话）
    ///
    /// 执行过程：
    ///   1. drain_all() → 空 sessions → 立即返回 false
    ///   2. kill_all() → 空 sessions → 无 panic
    ///
    /// 预期结果：
    ///   - 正断言：drain_all() == false（无会话无数据）
    ///   - 负断言：drain_all() != true（确实没有 dirty 数据）
    #[test]
    fn test_drain_all_empty_returns_false_and_kill_all_no_panic() {
        let mut rt = Runtime::new();
        let dirty = rt.drain_all();
        assert!(!dirty, "空 Runtime drain_all 应返回 false（无会话无数据）");
        assert!(dirty == false, "drain_all 返回值应精确为 false，不是 true");
        rt.kill_all(); // 不应 panic
    }

    /// 两次 spawn_terminal 分配递增 session_id，两个节点均进入 document。
    ///
    /// 数据构造：
    ///   first:  bounds=(0,0,400,300), shell="/bin/sh", 80×24
    ///   second: bounds=(400,0,400,300), shell="/bin/sh", 80×24
    ///   next_session_id 初始=1，spawn 后递增：first→sid=1，second→sid=2
    ///
    /// 执行过程：
    ///   1. spawn_terminal(first)  → node_id_a=1, sid=1
    ///   2. spawn_terminal(second) → node_id_b=2, sid=2
    ///   3. 确认两个 session 均可查到，document 有 2 个节点
    ///
    /// 预期结果：
    ///   - 正断言：node_id_a != node_id_b（id 唯一）
    ///   - 正断言：session(1).is_some() && session(2).is_some()
    ///   - 正断言：document.nodes().len() == 2
    ///   - 负断言：document.nodes().len() != 1（不是只有一个节点）
    #[test]
    fn test_two_spawn_terminals_get_distinct_ids() {
        let mut rt = Runtime::new();
        let b1 = CanvasRect::new(0.0, 0.0, 400.0, 300.0);
        let b2 = CanvasRect::new(400.0, 0.0, 400.0, 300.0);

        let id_a = rt.spawn_terminal(b1, "/bin/sh", 80, 24).expect("第一个 spawn 应成功");
        let id_b = rt.spawn_terminal(b2, "/bin/sh", 80, 24).expect("第二个 spawn 应成功");

        assert_ne!(id_a, id_b, "两个 spawn_terminal 应返回不同的 node_id");
        assert!(rt.session(1).is_some(), "session_id=1 应已注册");
        assert!(rt.session(2).is_some(), "session_id=2 应已注册");
        assert_eq!(rt.document.nodes().len(), 2, "两次 spawn 后 document 应有 2 个节点");
        assert_ne!(rt.document.nodes().len(), 1, "不应只有 1 个节点（两次 spawn 均应成功）");

        rt.kill_all();
    }
}
