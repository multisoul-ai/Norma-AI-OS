//! Card registry: app 层按 NodeType 分发卡片渲染。

pub mod agent;
pub(crate) mod border_glow;
pub mod browser;
pub mod chrome;
pub(crate) mod intro;
pub mod preview;
pub(crate) mod snake;
pub mod term;

use crate::app::browser::{BrowserAddressInput, BrowserAuthUiState};
use crate::canvas::text_selection::TextSelection;
use crate::cards::term::render_cache::TerminalRenderCacheHandle;
use crate::motion::InitializationPresentation;
use crate::preview::{ImageViewerPresentation, PreviewLoadState};
use canvas::{CanvasNode, CanvasRect, NodeType};
use chrome::CardChrome;
use gpui::AnyElement;
use runtime::{NodeResize, Runtime};
use std::sync::Arc;
use ui::Theme;

pub trait Card: Sync {
    fn ty(&self) -> NodeType;
    fn chrome(&self, node: &CanvasNode, runtime: &Runtime) -> CardChrome;
    fn body(&self, node: &CanvasNode, context: CardBodyContext<'_>) -> AnyElement;
    fn resize(&self, bounds: CanvasRect, zoom: f32) -> Option<NodeResize>;
    fn wheel_scroll_mode(&self) -> CardWheelScrollMode {
        CardWheelScrollMode::CanvasViewport
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardWheelScrollMode {
    CanvasViewport,
    NativeElement,
    TerminalScrollback,
}

pub struct CardBodyContext<'a> {
    pub runtime: &'a Runtime,
    pub terminal_render_cache: TerminalRenderCacheHandle,
    pub preview_state: Option<&'a PreviewLoadState>,
    pub preview_editor: Option<&'a crate::preview::PreviewEditor>,
    pub preview_image_viewer: Option<ImageViewerPresentation>,
    pub preview_image_dragging: bool,
    pub text_selection: Option<TextSelection>,
    pub focused: bool,
    pub reduce_motion: bool,
    pub initialization: Option<InitializationPresentation>,
    pub terminal_cursor_phase: f32,
    pub browser_address_input: Option<gpui::Entity<BrowserAddressInput>>,
    pub browser_auth_state: BrowserAuthUiState,
    pub browser_loading: bool,
    pub ego_preview: Option<Arc<gpui::RenderImage>>,
    #[cfg(all(target_os = "macos", not(feature = "e2e-test-support")))]
    pub ego_live_preview: Option<core_video::pixel_buffer::CVPixelBuffer>,
    pub ego_preview_error: Option<String>,
    /// 卡片内容区是否被 GPUI 浮层（如布局菜单）遮挡。遮挡时浏览器卡片应渲染
    /// 中性背景，避免显示 "Loading browser..." 占位文字。
    pub occluded_by_overlay: bool,
    pub theme: Theme,
    pub zoom: f32,
    pub bottom_radius_px: f32,
}

pub fn card_for(ty: NodeType) -> &'static dyn Card {
    let card: &'static dyn Card = match ty {
        NodeType::Term => &term::card::CARD,
        NodeType::Agent => &agent::card::CARD,
        NodeType::Preview => &preview::card::CARD,
        NodeType::Browser => &browser::card::CARD,
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
    ///   NodeType::Rect    → chrome.rs 的 EmptyCard
    ///   NodeType::Browser → cards/browser/card.rs 的 BrowserCard
    ///   五种类型是一一映射，registry 不做运行时动态注册
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
        let preview = card_for(NodeType::Preview);
        let browser = card_for(NodeType::Browser);

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
        assert_eq!(
            preview.ty(),
            NodeType::Preview,
            "NodeType::Preview 应 dispatch 到 PreviewCard"
        );
        assert_eq!(
            browser.ty(),
            NodeType::Browser,
            "NodeType::Browser 应 dispatch 到 BrowserCard"
        );
        assert_ne!(
            term.ty(),
            NodeType::Agent,
            "Term card 不应误返回 Agent 类型，否则 registry dispatch 错误"
        );
        assert_eq!(
            preview.wheel_scroll_mode(),
            CardWheelScrollMode::NativeElement,
            "Preview card 应声明 NativeElement 模式，供 GPUI overflow_scroll 接管滚轮"
        );
        assert_eq!(
            browser.wheel_scroll_mode(),
            CardWheelScrollMode::NativeElement,
            "Browser card 应声明 NativeElement 模式，供 native WebView 接管滚轮"
        );
        assert_eq!(
            term.wheel_scroll_mode(),
            CardWheelScrollMode::TerminalScrollback,
            "Term card 应声明 TerminalScrollback 模式，滚轮用于本地回滚"
        );
        assert_eq!(
            agent.wheel_scroll_mode(),
            CardWheelScrollMode::TerminalScrollback,
            "Agent card 应复用 terminal scrollback 模式，覆盖 Claude/Codex"
        );
        assert_eq!(
            rect.wheel_scroll_mode(),
            CardWheelScrollMode::CanvasViewport,
            "Rect card 不支持内部滚动，应保留 CanvasViewport 模式"
        );
    }
}

#[cfg(test)]
#[path = "../../tests/unit/cards/browser_url_tests.rs"]
mod browser_url_tests;
