//! 纯路由：根据当前输入模式 + 事件，决定动作。无副作用，便于 TDD。

use crate::focus::InputMode;
use canvas::NodeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    /// 单击，hit 为命中的节点（None = 空白）。
    Click { hit: Option<NodeId> },
    /// 双击，is_terminal 表示命中的是否为终端节点。
    DoubleClick { hit: Option<NodeId>, is_terminal: bool },
    /// 一次键盘按键（具体编码在 terminal::input 完成）。
    Key,
    /// Esc。
    Escape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    SelectNode(NodeId),
    ClearSelection,
    FocusTerminal(NodeId),
    SendKeyToTerminal(NodeId),
    ExitTerminalFocus,
    Ignore,
}

pub fn route(mode: &InputMode, event: &InputEvent) -> Route {
    match mode {
        InputMode::CanvasSelect => match event {
            InputEvent::Click { hit: Some(id) } => Route::SelectNode(*id),
            InputEvent::Click { hit: None } => Route::ClearSelection,
            InputEvent::DoubleClick { hit: Some(id), is_terminal: true } => Route::FocusTerminal(*id),
            InputEvent::DoubleClick { hit: Some(id), is_terminal: false } => Route::SelectNode(*id),
            InputEvent::DoubleClick { hit: None, .. } => Route::ClearSelection,
            InputEvent::Key => Route::Ignore,
            InputEvent::Escape => Route::Ignore,
        },
        InputMode::TerminalFocused(fid) => match event {
            InputEvent::Escape => Route::ExitTerminalFocus,
            InputEvent::Key => Route::SendKeyToTerminal(*fid),
            InputEvent::Click { hit: Some(id) } => Route::SelectNode(*id),
            InputEvent::Click { hit: None } => Route::ClearSelection,
            InputEvent::DoubleClick { hit: Some(id), is_terminal: true } => Route::FocusTerminal(*id),
            InputEvent::DoubleClick { hit: Some(id), is_terminal: false } => Route::SelectNode(*id),
            InputEvent::DoubleClick { hit: None, .. } => Route::ClearSelection,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 画布模式下单击命中节点 → 选中该节点。
    ///
    /// 预期结果：
    ///   - 正断言：CanvasSelect + Click(hit=5) → SelectNode(5)
    ///   - 负断言：结果不是 ClearSelection（命中了就不该清空）
    #[test]
    fn test_canvas_click_hit_selects() {
        let r = route(&InputMode::CanvasSelect, &InputEvent::Click { hit: Some(5) });
        assert_eq!(r, Route::SelectNode(5), "命中节点单击应 SelectNode(5)");
        assert_ne!(r, Route::ClearSelection, "命中时不应清空选择");
    }

    /// 画布模式下点空白 → 清空选择。
    ///
    /// 预期结果：
    ///   - 正断言：CanvasSelect + Click(hit=None) → ClearSelection
    ///   - 负断言：结果不是 SelectNode（未命中任何节点，不应选中）
    #[test]
    fn test_canvas_click_empty_clears() {
        let r = route(&InputMode::CanvasSelect, &InputEvent::Click { hit: None });
        assert_eq!(r, Route::ClearSelection, "点空白应清空选择");
        assert_ne!(r, Route::SelectNode(0), "未命中节点时不应产生 SelectNode");
    }

    /// 画布模式下双击终端节点 → 聚焦终端。
    ///
    /// 预期结果：
    ///   - 正断言：DoubleClick(hit=9, is_terminal=true) → FocusTerminal(9)
    ///   - 负断言：非终端双击不聚焦（→ SelectNode）
    #[test]
    fn test_canvas_double_click_terminal_focuses() {
        let r = route(&InputMode::CanvasSelect, &InputEvent::DoubleClick { hit: Some(9), is_terminal: true });
        assert_eq!(r, Route::FocusTerminal(9), "双击终端节点应聚焦终端");

        let r2 = route(&InputMode::CanvasSelect, &InputEvent::DoubleClick { hit: Some(9), is_terminal: false });
        assert_eq!(r2, Route::SelectNode(9), "双击非终端节点应只选中");
    }

    /// 终端聚焦模式下按键 → 发送到该终端；Esc → 退出聚焦。
    ///
    /// 预期结果：
    ///   - 正断言：TerminalFocused(7) + Key → SendKeyToTerminal(7)
    ///   - 正断言：TerminalFocused(7) + Escape → ExitTerminalFocus
    ///   - 负断言：聚焦态下 Key 不应是 ClearSelection（不能误清画布选择）
    #[test]
    fn test_terminal_focus_routes_keys_and_escape() {
        let key = route(&InputMode::TerminalFocused(7), &InputEvent::Key);
        assert_eq!(key, Route::SendKeyToTerminal(7), "聚焦态按键应发往终端 7");
        assert_ne!(key, Route::ClearSelection, "聚焦态按键不应清空画布选择");

        let esc = route(&InputMode::TerminalFocused(7), &InputEvent::Escape);
        assert_eq!(esc, Route::ExitTerminalFocus, "聚焦态 Esc 应退出终端聚焦");
    }

    /// 画布模式下按键被忽略（没有聚焦终端，键无处可去）。
    ///
    /// 预期结果：
    ///   - 正断言：CanvasSelect + Key → Ignore
    ///   - 负断言：不应是 SendKeyToTerminal（画布模式无终端接收按键）
    #[test]
    fn test_canvas_key_ignored() {
        let r = route(&InputMode::CanvasSelect, &InputEvent::Key);
        assert_eq!(r, Route::Ignore, "画布模式下普通按键应忽略");
        assert_ne!(r, Route::SendKeyToTerminal(0), "画布模式按键不应路由到任何终端");
    }

    /// 画布模式下 Esc 也被忽略（未聚焦终端时 Esc 无意义）。
    ///
    /// 预期结果：
    ///   - 正断言：CanvasSelect + Escape → Ignore
    ///   - 负断言：不应是 ExitTerminalFocus（未处于终端聚焦态）
    #[test]
    fn test_canvas_escape_ignored() {
        let r = route(&InputMode::CanvasSelect, &InputEvent::Escape);
        assert_eq!(r, Route::Ignore, "画布模式下 Esc 应被忽略");
        assert_ne!(r, Route::ExitTerminalFocus, "画布模式无聚焦终端，Esc 不应产生 ExitTerminalFocus");
    }

    /// 终端聚焦模式下双击空白 → 清空选择并隐式退出（ClearSelection，不是 ExitTerminalFocus）。
    ///
    /// 预期结果：
    ///   - 正断言：TerminalFocused(3) + DoubleClick(hit=None) → ClearSelection
    ///   - 负断言：不是 ExitTerminalFocus（双击空白不算显式退出终端）
    #[test]
    fn test_terminal_focus_double_click_empty_clears() {
        let r = route(&InputMode::TerminalFocused(3), &InputEvent::DoubleClick { hit: None, is_terminal: false });
        assert_eq!(r, Route::ClearSelection, "聚焦态双击空白应产生 ClearSelection");
        assert_ne!(r, Route::ExitTerminalFocus, "双击空白不是显式退出聚焦，不应产生 ExitTerminalFocus");
    }
}
