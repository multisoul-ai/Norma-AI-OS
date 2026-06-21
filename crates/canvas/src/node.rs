//! 画布节点数据模型（零 gpui）。

use crate::geometry::CanvasRect;

pub type NodeId = u64;

/// 普通矩形节点（本切片用于验证拖拽/选中）。color 为 0xRRGGBB。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectNode {
    pub color: u32,
}

/// 终端节点的可持久化描述。实际 PTY/session 存在 runtime 层。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TermSpec;

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

/// Agent 节点的可持久化描述。实际进程/session 存在 runtime 层。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentSpec {
    pub profile: AgentProfile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeType {
    Rect,
    Term,
    Agent,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NodeKind {
    Rect(RectNode),
    Term(TermSpec),
    Agent(AgentSpec),
}

/// 兼容旧调用方的类型名；新代码优先使用 NodeKind。
pub type CanvasNodeKind = NodeKind;

impl NodeKind {
    pub fn node_type(self) -> NodeType {
        match self {
            NodeKind::Rect(_) => NodeType::Rect,
            NodeKind::Term(_) => NodeType::Term,
            NodeKind::Agent(_) => NodeType::Agent,
        }
    }

    pub fn display_title(self) -> &'static str {
        match self {
            NodeKind::Rect(_) => "Node",
            NodeKind::Term(_) => "Terminal",
            NodeKind::Agent(node) => node.profile.display_title(),
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

    /// Agent 节点只保存 profile 描述，并按 profile 暴露显示标题与类型。
    ///
    /// 数据构造（含关键数值推导）：
    ///   Claude Agent: profile=Claude
    ///   Codex Agent:  profile=Codex
    ///   runtime session 不在 canvas 层构造，因此这里没有 session_id 字段
    ///
    /// 执行过程：
    ///   1. 构造 NodeKind::Agent(Claude)
    ///   2. 构造 NodeKind::Agent(Codex)
    ///   3. 调用 node_type / display_title
    ///
    /// 预期结果：
    ///   - 正断言：Claude/Codex Agent 类型都是 NodeType::Agent
    ///   - 正断言：标题分别为 "Claude" 和 "Codex"
    ///   - 负断言：Agent 标题不应退化为普通 "Terminal"
    #[test]
    fn test_agent_kind_is_description_only_and_exposes_profile_title() {
        let claude = NodeKind::Agent(AgentSpec {
            profile: AgentProfile::Claude,
        });
        let codex = NodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
        });

        assert_eq!(
            claude.node_type(),
            NodeType::Agent,
            "Claude Agent 应暴露 NodeType::Agent，供 runtime/app registry dispatch"
        );
        assert_eq!(
            codex.node_type(),
            NodeType::Agent,
            "Codex Agent 应暴露 NodeType::Agent，供 runtime/app registry dispatch"
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

    /// Term/Rect 节点只暴露节点类型和默认标题，不暴露 runtime session。
    ///
    /// 数据构造（含关键数值推导）：
    ///   Term: TermSpec（空描述，runtime 负责实际 session）
    ///   Rect: color=0x4080ff（颜色不影响 terminal helper）
    ///
    /// 执行过程：
    ///   1. 构造 NodeKind::Term(TermSpec)
    ///   2. 构造 NodeKind::Rect
    ///   3. 分别调用 node_type / display_title
    ///
    /// 预期结果：
    ///   - 正断言：Term 类型是 NodeType::Term，标题为 "Terminal"
    ///   - 正断言：Rect 类型是 NodeType::Rect，标题为 "Node"
    ///   - 负断言：Rect 类型不应是 NodeType::Term
    #[test]
    fn test_node_type_and_titles_for_term_and_rect_nodes() {
        let term = NodeKind::Term(TermSpec);
        let rect = NodeKind::Rect(RectNode { color: 0x4080ff });

        assert_eq!(
            term.node_type(),
            NodeType::Term,
            "Term 节点应暴露 NodeType::Term，供 card registry dispatch"
        );
        assert_eq!(
            term.display_title(),
            "Terminal",
            "Term 节点默认标题应保持 Terminal"
        );
        assert_eq!(
            rect.node_type(),
            NodeType::Rect,
            "Rect 节点应暴露 NodeType::Rect"
        );
        assert_eq!(rect.display_title(), "Node", "Rect 节点默认标题应保持 Node");
        assert_ne!(
            rect.node_type(),
            NodeType::Term,
            "Rect 节点不应伪装成 Term，否则 app 会错误选择 terminal card"
        );
    }
}
