//! Agent card renderer.

use crate::app::MultisoulApp;
use crate::cards::chrome::CardChrome;
use crate::cards::term::card::term_resize;
use crate::cards::term::view::render_terminal_node;
use crate::cards::Card;
use canvas::{CanvasNode, CanvasNodeKind, CanvasRect, NodeType};
use gpui::{AnyElement, Context, Window};
use runtime::{NodeResize, Runtime};
use ui::Theme;

pub struct AgentCard;

pub static CARD: AgentCard = AgentCard;

impl Card for AgentCard {
    fn ty(&self) -> NodeType {
        NodeType::Agent
    }

    fn chrome(&self, node: &CanvasNode, _runtime: &Runtime) -> CardChrome {
        let title = match node.kind {
            CanvasNodeKind::Agent(agent) => agent.profile.display_title(),
            _ => "Agent",
        };
        CardChrome { title }
    }

    fn body(
        &self,
        node: &CanvasNode,
        runtime: &Runtime,
        theme: Theme,
        zoom: f32,
        bottom_radius_px: f32,
        _window: &mut Window,
        _cx: &mut Context<MultisoulApp>,
    ) -> AnyElement {
        render_terminal_node(runtime.node_view(node.id), theme, zoom, bottom_radius_px)
    }

    fn resize(&self, bounds: CanvasRect, zoom: f32) -> Option<NodeResize> {
        Some(term_resize(bounds, zoom))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use canvas::{AgentProfile, AgentSpec, CanvasRect};

    /// AgentCard chrome 使用 Agent profile 作为标题。
    ///
    /// 数据构造（含关键数值推导）：
    ///   Claude node: id=1, kind=Agent(profile=Claude)
    ///   Codex node:  id=2, kind=Agent(profile=Codex)
    ///   profile display title 分别为 "Claude" / "Codex"
    ///
    /// 执行过程：
    ///   1. 构造两个 CanvasNode
    ///   2. 调用 AgentCard.chrome
    ///
    /// 预期结果：
    ///   - 正断言：Claude/Codex title 分别正确
    ///   - 负断言：Claude title 不应退化为 "Agent" 或 "Terminal"
    #[test]
    fn test_agent_card_chrome_uses_profile_title() {
        let runtime = Runtime::new();
        let claude = CanvasNode {
            id: 1,
            kind: CanvasNodeKind::Agent(AgentSpec {
                profile: AgentProfile::Claude,
            }),
            bounds: CanvasRect::new(0.0, 0.0, 100.0, 80.0),
            z_index: 1,
            selected: false,
        };
        let codex = CanvasNode {
            id: 2,
            kind: CanvasNodeKind::Agent(AgentSpec {
                profile: AgentProfile::Codex,
            }),
            bounds: CanvasRect::new(0.0, 0.0, 100.0, 80.0),
            z_index: 2,
            selected: false,
        };

        let claude_chrome = CARD.chrome(&claude, &runtime);
        let codex_chrome = CARD.chrome(&codex, &runtime);

        assert_eq!(
            claude_chrome.title, "Claude",
            "Claude Agent card 标题应显示 Claude"
        );
        assert_eq!(
            codex_chrome.title, "Codex",
            "Codex Agent card 标题应显示 Codex"
        );
        assert_ne!(
            claude_chrome.title, "Agent",
            "Claude Agent card 不应退化为泛化 Agent 标题"
        );
        assert_ne!(
            claude_chrome.title, "Terminal",
            "Claude Agent card 不应退化为 Terminal 标题"
        );
    }
}
