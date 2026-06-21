//! 画布节点数据模型（零 gpui）。

use crate::geometry::CanvasRect;

pub type NodeId = u64;

/// 普通矩形节点（本切片用于验证拖拽/选中）。color 为 0xRRGGBB。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectNode {
    pub color: u32,
}

/// 指向一个 terminal session 的引用（实际 session 存在 runtime 层）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerminalNodeRef {
    pub session_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentProfile {
    Claude,
    Codex,
}

impl AgentProfile {
    pub fn display_title(self) -> &'static str {
        match self {
            AgentProfile::Claude => "Claude",
            AgentProfile::Codex => "Codex",
        }
    }
}

/// 指向一个 agent terminal session，并保留 agent profile 供 UI 命名。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentNodeRef {
    pub profile: AgentProfile,
    pub session_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CanvasNodeKind {
    Rect(RectNode),
    Terminal(TerminalNodeRef),
    Agent(AgentNodeRef),
}

impl CanvasNodeKind {
    pub fn terminal_session_id(self) -> Option<u64> {
        match self {
            CanvasNodeKind::Terminal(node) => Some(node.session_id),
            CanvasNodeKind::Agent(node) => Some(node.session_id),
            CanvasNodeKind::Rect(_) => None,
        }
    }

    pub fn is_terminal_backed(self) -> bool {
        self.terminal_session_id().is_some()
    }

    pub fn display_title(self) -> &'static str {
        match self {
            CanvasNodeKind::Rect(_) => "Node",
            CanvasNodeKind::Terminal(_) => "Terminal",
            CanvasNodeKind::Agent(node) => node.profile.display_title(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasNode {
    pub id: NodeId,
    pub kind: CanvasNodeKind,
    pub bounds: CanvasRect,
    pub z_index: i32,
    pub selected: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Agent 节点复用终端能力，并按 profile 暴露显示标题。
    ///
    /// 数据构造（含关键数值推导）：
    ///   Claude Agent: profile=Claude, session_id=7
    ///   Codex Agent:  profile=Codex,  session_id=8
    ///   session_id 直接来自 runtime 分配；这里固定为 7/8 便于断言 helper 不改写 id
    ///
    /// 执行过程：
    ///   1. 构造 CanvasNodeKind::Agent(Claude, session_id=7)
    ///   2. 构造 CanvasNodeKind::Agent(Codex, session_id=8)
    ///   3. 调用 is_terminal_backed / terminal_session_id / display_title
    ///
    /// 预期结果：
    ///   - 正断言：Claude/Codex Agent 都是 terminal-backed
    ///   - 正断言：Claude session_id 保持为 7，Codex session_id 保持为 8
    ///   - 正断言：标题分别为 "Claude" 和 "Codex"
    ///   - 负断言：Agent 标题不应退化为普通 "Terminal"
    #[test]
    fn test_agent_kind_is_terminal_backed_and_exposes_profile_title() {
        let claude = CanvasNodeKind::Agent(AgentNodeRef {
            profile: AgentProfile::Claude,
            session_id: 7,
        });
        let codex = CanvasNodeKind::Agent(AgentNodeRef {
            profile: AgentProfile::Codex,
            session_id: 8,
        });

        assert!(
            claude.is_terminal_backed(),
            "Claude Agent 应复用终端 session，is_terminal_backed 必须为 true"
        );
        assert!(
            codex.is_terminal_backed(),
            "Codex Agent 应复用终端 session，is_terminal_backed 必须为 true"
        );
        assert_eq!(
            claude.terminal_session_id(),
            Some(7),
            "Claude Agent 应暴露原始 session_id=7，便于 app 渲染对应 PTY"
        );
        assert_eq!(
            codex.terminal_session_id(),
            Some(8),
            "Codex Agent 应暴露原始 session_id=8，便于 app 渲染对应 PTY"
        );
        assert_eq!(
            claude.display_title(),
            "Claude",
            "Claude Agent 标题应显示为 Claude"
        );
        assert_eq!(
            codex.display_title(),
            "Codex",
            "Codex Agent 标题应显示为 Codex"
        );
        assert_ne!(
            claude.display_title(),
            "Terminal",
            "Claude Agent 不应退化显示为普通 Terminal"
        );
        assert_ne!(
            codex.display_title(),
            "Terminal",
            "Codex Agent 不应退化显示为普通 Terminal"
        );
    }

    /// 普通 Terminal 节点仍暴露终端 session，Rect 节点不暴露 session。
    ///
    /// 数据构造（含关键数值推导）：
    ///   Terminal: session_id=3
    ///   Rect: color=0x4080ff（颜色不影响 terminal helper）
    ///
    /// 执行过程：
    ///   1. 构造 CanvasNodeKind::Terminal(session_id=3)
    ///   2. 构造 CanvasNodeKind::Rect
    ///   3. 分别调用 helper
    ///
    /// 预期结果：
    ///   - 正断言：Terminal 是 terminal-backed 且 session_id=3
    ///   - 正断言：Terminal 标题仍为 "Terminal"
    ///   - 负断言：Rect 不是 terminal-backed，且没有 session_id
    #[test]
    fn test_terminal_helpers_preserve_terminal_and_exclude_rect_nodes() {
        let terminal = CanvasNodeKind::Terminal(TerminalNodeRef { session_id: 3 });
        let rect = CanvasNodeKind::Rect(RectNode { color: 0x4080ff });

        assert!(
            terminal.is_terminal_backed(),
            "普通 Terminal 节点应继续被识别为 terminal-backed"
        );
        assert_eq!(
            terminal.terminal_session_id(),
            Some(3),
            "普通 Terminal 节点应暴露 session_id=3"
        );
        assert_eq!(
            terminal.display_title(),
            "Terminal",
            "普通 Terminal 节点标题应保持 Terminal"
        );
        assert!(
            !rect.is_terminal_backed(),
            "Rect 节点不是可运行终端节点，is_terminal_backed 应为 false"
        );
        assert_eq!(
            rect.terminal_session_id(),
            None,
            "Rect 节点不应暴露任何 terminal session id"
        );
    }
}
