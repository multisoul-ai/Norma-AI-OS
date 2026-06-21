//! 节点生命周期 + 终端会话注册表。把 canvas 文档与 terminal 会话组装在一起。

use crate::focus::{FocusState, InputMode};
use canvas::{
    AgentNodeRef, AgentProfile, CanvasNodeKind, CanvasRect, Document, NodeId, Selection,
    TerminalNodeRef,
};
use std::collections::HashMap;
use terminal::{PtyCommand, PtyError, TerminalSession, TerminalSessionId};

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
        let node_id = self.document.add_node(
            CanvasNodeKind::Terminal(TerminalNodeRef { session_id: sid }),
            bounds,
        );
        Ok(node_id)
    }

    pub fn spawn_agent(
        &mut self,
        profile: AgentProfile,
        bounds: CanvasRect,
        cols: u16,
        rows: u16,
    ) -> Result<NodeId, PtyError> {
        let command = Self::agent_command(profile);
        self.spawn_agent_with_command(profile, bounds, command, cols, rows)
    }

    fn spawn_agent_with_command(
        &mut self,
        profile: AgentProfile,
        bounds: CanvasRect,
        command: PtyCommand,
        cols: u16,
        rows: u16,
    ) -> Result<NodeId, PtyError> {
        let sid = self.next_session_id;
        self.next_session_id += 1;
        let session = TerminalSession::spawn_command(sid, command, cols, rows)?;
        self.sessions.insert(sid, session);
        let node_id = self.document.add_node(
            CanvasNodeKind::Agent(AgentNodeRef {
                profile,
                session_id: sid,
            }),
            bounds,
        );
        Ok(node_id)
    }

    fn agent_command(profile: AgentProfile) -> PtyCommand {
        match profile {
            AgentProfile::Claude => PtyCommand::new("/bin/zsh")
                .args(["-ic", "exec claude --dangerously-skip-permissions"]),
            AgentProfile::Codex => PtyCommand::new("/bin/zsh").args([
                "-ic",
                "exec codex --dangerously-bypass-approvals-and-sandbox",
            ]),
        }
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

    /// 删除节点并清理与该节点绑定的 runtime 状态。
    pub fn remove_node(&mut self, node_id: NodeId) -> bool {
        let Some(node) = self.document.remove_node(node_id) else {
            return false;
        };

        if let Some(session_id) = node.kind.terminal_session_id() {
            if let Some(mut session) = self.sessions.remove(&session_id) {
                session.kill();
            }
        }

        self.selection.deselect(node_id);
        if matches!(self.focus.mode(), InputMode::TerminalFocused(focused) if focused == node_id) {
            self.focus.exit_to_canvas();
        }

        true
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
    use canvas::{geometry::CanvasRect, AgentProfile};
    use terminal::PtyCommand;

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
        assert_ne!(
            rt.document.nodes().len(),
            1,
            "未添加节点，文档不应有 1 个节点"
        );
        assert_eq!(
            rt.focus.mode(),
            InputMode::CanvasSelect,
            "新建 Runtime 焦点应为 CanvasSelect"
        );
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
        assert!(
            rt.session(1).is_some(),
            "session_id=1 应已注册到 sessions map"
        );
        assert!(
            rt.session(99).is_none(),
            "session_id=99 未创建，应返回 None"
        );

        assert_eq!(
            rt.document.nodes().len(),
            1,
            "spawn 后 document 应恰好有 1 个节点"
        );

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

        let id_a = rt
            .spawn_terminal(b1, "/bin/sh", 80, 24)
            .expect("第一个 spawn 应成功");
        let id_b = rt
            .spawn_terminal(b2, "/bin/sh", 80, 24)
            .expect("第二个 spawn 应成功");

        assert_ne!(id_a, id_b, "两个 spawn_terminal 应返回不同的 node_id");
        assert!(rt.session(1).is_some(), "session_id=1 应已注册");
        assert!(rt.session(2).is_some(), "session_id=2 应已注册");
        assert_eq!(
            rt.document.nodes().len(),
            2,
            "两次 spawn 后 document 应有 2 个节点"
        );
        assert_ne!(
            rt.document.nodes().len(),
            1,
            "不应只有 1 个节点（两次 spawn 均应成功）"
        );

        rt.kill_all();
    }

    /// spawn_agent_with_command 同时注册会话并在 document 中创建 Agent 节点。
    ///
    /// 数据构造（含关键数值推导）：
    ///   bounds = CanvasRect::new(0, 0, 640, 400)
    ///   profile = AgentProfile::Claude
    ///   command = /bin/sh -lc "printf AGENT_OK; exit"（测试替身，避免依赖本机 claude）
    ///   cols=80, rows=24
    ///   next_session_id 初始=1，因此本次 Agent session_id=1，node_id=1
    ///
    /// 执行过程：
    ///   1. spawn_agent_with_command(Claude, bounds, command, 80, 24)
    ///      → TerminalSession::spawn_command(1, command, 80, 24)
    ///      → sessions.insert(1, session)
    ///      → document.add_node(Agent(profile=Claude, session_id=1), bounds)
    ///   2. 查询 session(1) 与 document.node(node_id)
    ///   3. 检查节点 kind/profile/session_id
    ///
    /// 预期结果：
    ///   - 正断言：spawn_agent_with_command 返回 Ok(node_id)
    ///   - 正断言：session(1) 已注册
    ///   - 正断言：document 节点是 Agent 且 profile=Claude、session_id=1
    ///   - 负断言：节点不应是普通 Terminal
    #[test]
    fn test_spawn_agent_registers_session_and_agent_node() {
        let mut rt = Runtime::new();
        let bounds = CanvasRect::new(0.0, 0.0, 640.0, 400.0);
        let command = PtyCommand::new("/bin/sh").args(["-lc", "printf AGENT_OK; exit"]);

        let node_id = rt
            .spawn_agent_with_command(AgentProfile::Claude, bounds, command, 80, 24)
            .expect("spawn_agent_with_command 应成功");

        assert!(rt.session(1).is_some(), "Agent session_id=1 应已注册");
        assert_eq!(
            rt.document.nodes().len(),
            1,
            "spawn Agent 后 document 应恰好有 1 个节点"
        );

        let node = rt
            .document
            .node(node_id)
            .expect("spawn 返回的 node_id 应能在 document 中查到");
        match node.kind {
            CanvasNodeKind::Agent(agent) => {
                assert_eq!(
                    agent.profile,
                    AgentProfile::Claude,
                    "Agent 节点 profile 应保存为 Claude"
                );
                assert_eq!(agent.session_id, 1, "Agent 节点应指向刚注册的 session_id=1");
            }
            CanvasNodeKind::Terminal(_) => {
                panic!("spawn_agent_with_command 不应创建普通 Terminal 节点");
            }
            CanvasNodeKind::Rect(_) => {
                panic!("spawn_agent_with_command 不应创建 Rect 节点");
            }
        }

        rt.kill_all();
    }

    /// Claude/Codex Agent profile 映射到精确的高权限启动命令。
    ///
    /// 数据构造（含关键数值推导）：
    ///   Claude profile → /bin/zsh -ic "exec claude --dangerously-skip-permissions"
    ///   Codex profile  → /bin/zsh -ic "exec codex --dangerously-bypass-approvals-and-sandbox"
    ///   使用 -ic：交互 shell 读取用户环境，exec 替换 shell 进程
    ///
    /// 执行过程：
    ///   1. Runtime::agent_command(Claude)
    ///   2. Runtime::agent_command(Codex)
    ///   3. 分别检查 program 与 args
    ///
    /// 预期结果：
    ///   - 正断言：两个命令 program 都是 /bin/zsh
    ///   - 正断言：Claude/Codex args 精确匹配计划中的危险权限命令
    ///   - 负断言：Claude 命令不应包含 Codex 可执行名，Codex 命令不应包含 Claude 可执行名
    #[test]
    fn test_agent_command_specs_are_exact() {
        let claude = Runtime::agent_command(AgentProfile::Claude);
        let codex = Runtime::agent_command(AgentProfile::Codex);

        assert_eq!(
            claude.program, "/bin/zsh",
            "Claude Agent 应通过 /bin/zsh 启动以继承用户 shell 环境"
        );
        assert_eq!(
            codex.program, "/bin/zsh",
            "Codex Agent 应通过 /bin/zsh 启动以继承用户 shell 环境"
        );
        assert_eq!(
            claude.args,
            vec!["-ic", "exec claude --dangerously-skip-permissions"],
            "Claude Agent args 必须精确匹配计划中的高权限命令"
        );
        assert_eq!(
            codex.args,
            vec![
                "-ic",
                "exec codex --dangerously-bypass-approvals-and-sandbox"
            ],
            "Codex Agent args 必须精确匹配计划中的高权限命令"
        );
        assert!(
            !claude.args.iter().any(|arg| arg.contains("codex")),
            "Claude Agent 命令不应包含 codex 可执行名"
        );
        assert!(
            !codex.args.iter().any(|arg| arg.contains("claude")),
            "Codex Agent 命令不应包含 claude 可执行名"
        );
    }

    /// remove_node 删除 terminal-backed 卡片时同步清理会话、选择与焦点。
    ///
    /// 数据构造（含关键数值推导）：
    ///   first:  bounds=(0,0,400,300), session_id=1, node_id=1
    ///   second: bounds=(400,0,400,300), session_id=2, node_id=2
    ///   next_session_id 初始=1，连续 spawn_terminal 后 first→1、second→2
    ///
    /// 执行过程：
    ///   1. spawn first/second → document=[first, second], sessions={1,2}
    ///   2. selection.select_only(first)，focus_terminal(first)
    ///   3. remove_node(first) → 删除 document 节点，kill/drop session_id=1，清除 first 选择，退出焦点
    ///   4. 查询 first/second 的节点、session、选择与焦点状态
    ///
    /// 预期结果：
    ///   - 正断言：remove_node(first) 返回 true，first 节点和 session_id=1 均不存在
    ///   - 正断言：second 节点与 session_id=2 仍存在
    ///   - 正断言：删除聚焦节点后 focus 回到 CanvasSelect
    ///   - 负断言：selection 不再包含 first，remove_node(999) 返回 false
    #[test]
    fn test_remove_node_cleans_terminal_session_selection_and_focus() {
        use crate::focus::InputMode;

        let mut rt = Runtime::new();
        let first_bounds = CanvasRect::new(0.0, 0.0, 400.0, 300.0);
        let second_bounds = CanvasRect::new(400.0, 0.0, 400.0, 300.0);

        let first = rt
            .spawn_terminal(first_bounds, "/bin/sh", 80, 24)
            .expect("first spawn_terminal 应成功");
        let second = rt
            .spawn_terminal(second_bounds, "/bin/sh", 80, 24)
            .expect("second spawn_terminal 应成功");
        rt.selection.select_only(first);
        rt.focus.focus_terminal(first);

        let removed = rt.remove_node(first);

        assert!(
            removed,
            "remove_node(first) 应返回 true，表示确实删除了节点"
        );
        assert!(
            rt.document.node(first).is_none(),
            "first 节点应已从 document 删除，卡片不应继续渲染"
        );
        assert!(
            rt.session(1).is_none(),
            "first 对应的 session_id=1 应被移除，避免关闭卡片后子进程继续存活"
        );
        assert!(
            rt.document.node(second).is_some(),
            "删除 first 不应影响 second 节点"
        );
        assert!(
            rt.session(2).is_some(),
            "删除 first 不应移除 second 对应的 session_id=2"
        );
        assert!(
            !rt.selection.contains(first),
            "selection 不应继续包含已删除的 first 节点"
        );
        assert!(
            rt.selection.is_empty(),
            "删除唯一选中节点后 selection 应为空"
        );
        assert_eq!(
            rt.focus.mode(),
            InputMode::CanvasSelect,
            "删除聚焦终端节点后焦点应回到 CanvasSelect"
        );

        let missing_removed = rt.remove_node(999);
        assert!(
            !missing_removed,
            "remove_node(999) 应返回 false，不存在节点不能报告删除成功"
        );

        rt.kill_all();
    }
}
