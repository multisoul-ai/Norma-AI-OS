//! Card registry: app 层按 NodeType 分发卡片渲染。

pub mod agent;
pub mod chrome;
pub mod term;

use canvas::{CanvasNode, CanvasRect, NodeType};
use chrome::CardChrome;
use gpui::AnyElement;
use runtime::{NodeResize, Runtime};
use ui::Theme;

pub trait Card: Sync {
    fn ty(&self) -> NodeType;
    fn chrome(&self, node: &CanvasNode, runtime: &Runtime) -> CardChrome;
    fn body(&self, node: &CanvasNode, context: CardBodyContext<'_>) -> AnyElement;
    fn resize(&self, bounds: CanvasRect, zoom: f32) -> Option<NodeResize>;
}

pub struct CardBodyContext<'a> {
    pub runtime: &'a Runtime,
    pub theme: Theme,
    pub zoom: f32,
    pub bottom_radius_px: f32,
}

pub fn card_for(ty: NodeType) -> &'static dyn Card {
    let card: &'static dyn Card = match ty {
        NodeType::Term => &term::card::CARD,
        NodeType::Agent => &agent::card::CARD,
        NodeType::Rect => &chrome::EMPTY_CARD,
    };
    debug_assert_eq!(
        card.ty(),
        ty,
        "card registry returned a renderer for the wrong node type"
    );
    card
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Card registry 按 NodeType 返回对应卡片 renderer。
    ///
    /// 数据构造（含关键数值推导）：
    ///   NodeType::Term  → cards/term/card.rs 的 TermCard
    ///   NodeType::Agent → cards/agent/card.rs 的 AgentCard
    ///   NodeType::Rect  → chrome.rs 的 EmptyCard
    ///   三种类型是一一映射，registry 不做运行时动态注册
    ///
    /// 执行过程：
    ///   1. card_for(NodeType::Term)
    ///   2. card_for(NodeType::Agent)
    ///   3. card_for(NodeType::Rect)
    ///   4. 调用 ty() 验证返回值
    ///
    /// 预期结果：
    ///   - 正断言：Term/Agent/Rect 都返回同类型 card
    ///   - 负断言：Term card 不应误返回 Agent 类型
    #[test]
    fn test_card_registry_dispatches_by_node_type() {
        let term = card_for(NodeType::Term);
        let agent = card_for(NodeType::Agent);
        let rect = card_for(NodeType::Rect);

        assert_eq!(
            term.ty(),
            NodeType::Term,
            "NodeType::Term 应 dispatch 到 TermCard"
        );
        assert_eq!(
            agent.ty(),
            NodeType::Agent,
            "NodeType::Agent 应 dispatch 到 AgentCard"
        );
        assert_eq!(
            rect.ty(),
            NodeType::Rect,
            "NodeType::Rect 应 dispatch 到 EmptyCard"
        );
        assert_ne!(
            term.ty(),
            NodeType::Agent,
            "Term card 不应误返回 Agent 类型，否则 registry dispatch 错误"
        );
    }
}
