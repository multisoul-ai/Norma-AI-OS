//! 纯路由：根据当前输入模式 + 事件，决定动作。无副作用，便于 TDD。

use crate::focus::InputMode;
use canvas::NodeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    /// 单击，hit 为命中的节点（None = 空白）。
    Click { hit: Option<NodeId> },
    /// 双击或内容区激活，is_focusable 表示命中的节点是否可进入输入焦点。
    DoubleClick {
        hit: Option<NodeId>,
        is_focusable: bool,
    },
    /// 请求关闭/删除某个节点。
    CloseNode { node: NodeId },
    /// 一次键盘按键（具体编码在 app/card 层翻译为 NodeInput）。
    Key,
    /// 请求切换 master/detail 布局。active=true 表示当前已经在 master 模式。
    ToggleMasterLayout {
        active: bool,
        target: Option<NodeId>,
    },
    /// Esc。
    Escape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    SelectNode(NodeId),
    ClearSelection,
    FocusNode(NodeId),
    RemoveNode(NodeId),
    EnterMasterLayout(NodeId),
    ExitMasterLayout,
    SendInputToNode(NodeId),
    ExitNodeFocus,
    Ignore,
}

pub fn route(mode: &InputMode, event: &InputEvent) -> Route {
    if let InputEvent::ToggleMasterLayout { active, target } = event {
        return if *active {
            Route::ExitMasterLayout
        } else if let Some(node_id) = target {
            Route::EnterMasterLayout(*node_id)
        } else {
            Route::Ignore
        };
    }

    match mode {
        InputMode::CanvasSelect => match event {
            InputEvent::Click { hit: Some(id) } => Route::SelectNode(*id),
            InputEvent::Click { hit: None } => Route::ClearSelection,
            InputEvent::CloseNode { node } => Route::RemoveNode(*node),
            InputEvent::DoubleClick {
                hit: Some(id),
                is_focusable: true,
            } => Route::FocusNode(*id),
            InputEvent::DoubleClick {
                hit: Some(id),
                is_focusable: false,
            } => Route::SelectNode(*id),
            InputEvent::DoubleClick { hit: None, .. } => Route::ClearSelection,
            InputEvent::Key => Route::Ignore,
            InputEvent::ToggleMasterLayout { .. } => Route::Ignore,
            InputEvent::Escape => Route::Ignore,
        },
        InputMode::NodeFocused(fid) => match event {
            InputEvent::Escape => Route::ExitNodeFocus,
            InputEvent::Key => Route::SendInputToNode(*fid),
            InputEvent::Click { hit: Some(id) } => Route::SelectNode(*id),
            InputEvent::Click { hit: None } => Route::ClearSelection,
            InputEvent::CloseNode { node } => Route::RemoveNode(*node),
            InputEvent::DoubleClick {
                hit: Some(id),
                is_focusable: true,
            } => Route::FocusNode(*id),
            InputEvent::DoubleClick {
                hit: Some(id),
                is_focusable: false,
            } => Route::SelectNode(*id),
            InputEvent::DoubleClick { hit: None, .. } => Route::ClearSelection,
            InputEvent::ToggleMasterLayout { .. } => Route::Ignore,
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
        let r = route(
            &InputMode::CanvasSelect,
            &InputEvent::Click { hit: Some(5) },
        );
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

    /// 画布模式下激活可聚焦节点 → 聚焦节点；激活不可聚焦节点 → 选中节点。
    ///
    /// 数据构造（含关键数值推导）：
    ///   focusable node id = 9
    ///   non-focusable node id = 10
    ///   is_focusable 由 runtime Caps.focusable 提供，route 不自行查询 runtime
    ///
    /// 执行过程：
    ///   1. DoubleClick(hit=9, is_focusable=true) → route
    ///   2. DoubleClick(hit=10, is_focusable=false) → route
    ///
    /// 预期结果：
    ///   - 正断言：可聚焦节点激活后 FocusNode(9)
    ///   - 正断言：不可聚焦节点激活后 SelectNode(10)
    ///   - 负断言：不可聚焦节点不应产生 FocusNode
    #[test]
    fn test_canvas_double_click_focusable_node_focuses_otherwise_selects() {
        let focusable = route(
            &InputMode::CanvasSelect,
            &InputEvent::DoubleClick {
                hit: Some(9),
                is_focusable: true,
            },
        );
        let inert = route(
            &InputMode::CanvasSelect,
            &InputEvent::DoubleClick {
                hit: Some(10),
                is_focusable: false,
            },
        );

        assert_eq!(
            focusable,
            Route::FocusNode(9),
            "激活可聚焦节点应 FocusNode(9)"
        );
        assert_eq!(
            inert,
            Route::SelectNode(10),
            "激活不可聚焦节点应只选中 SelectNode(10)"
        );
        assert_ne!(inert, Route::FocusNode(10), "不可聚焦节点不应进入输入焦点");
    }

    /// 节点聚焦模式下按键 → 发送到该节点；Esc → 退出聚焦。
    ///
    /// 预期结果：
    ///   - 正断言：NodeFocused(7) + Key → SendInputToNode(7)
    ///   - 正断言：NodeFocused(7) + Escape → ExitNodeFocus
    ///   - 负断言：聚焦态下 Key 不应是 ClearSelection（不能误清画布选择）
    #[test]
    fn test_node_focus_routes_keys_and_escape() {
        let key = route(&InputMode::NodeFocused(7), &InputEvent::Key);
        assert_eq!(key, Route::SendInputToNode(7), "聚焦态按键应发往节点 7");
        assert_ne!(key, Route::ClearSelection, "聚焦态按键不应清空画布选择");

        let esc = route(&InputMode::NodeFocused(7), &InputEvent::Escape);
        assert_eq!(esc, Route::ExitNodeFocus, "聚焦态 Esc 应退出节点聚焦");
    }

    /// 画布模式下按键被忽略（没有聚焦节点，键无处可去）。
    ///
    /// 预期结果：
    ///   - 正断言：CanvasSelect + Key → Ignore
    ///   - 负断言：不应是 SendInputToNode（画布模式无节点接收按键）
    #[test]
    fn test_canvas_key_ignored() {
        let r = route(&InputMode::CanvasSelect, &InputEvent::Key);
        assert_eq!(r, Route::Ignore, "画布模式下普通按键应忽略");
        assert_ne!(
            r,
            Route::SendInputToNode(0),
            "画布模式按键不应路由到任何节点"
        );
    }

    /// 画布模式下 Esc 也被忽略（未聚焦节点时 Esc 无意义）。
    ///
    /// 预期结果：
    ///   - 正断言：CanvasSelect + Escape → Ignore
    ///   - 负断言：不应是 ExitNodeFocus（未处于节点聚焦态）
    #[test]
    fn test_canvas_escape_ignored() {
        let r = route(&InputMode::CanvasSelect, &InputEvent::Escape);
        assert_eq!(r, Route::Ignore, "画布模式下 Esc 应被忽略");
        assert_ne!(
            r,
            Route::ExitNodeFocus,
            "画布模式无聚焦节点，Esc 不应产生 ExitNodeFocus"
        );
    }

    /// 节点聚焦模式下双击空白 → 清空选择并隐式退出（ClearSelection，不是 ExitNodeFocus）。
    ///
    /// 预期结果：
    ///   - 正断言：NodeFocused(3) + DoubleClick(hit=None) → ClearSelection
    ///   - 负断言：不是 ExitNodeFocus（双击空白不算显式退出节点）
    #[test]
    fn test_node_focus_double_click_empty_clears() {
        let r = route(
            &InputMode::NodeFocused(3),
            &InputEvent::DoubleClick {
                hit: None,
                is_focusable: false,
            },
        );
        assert_eq!(
            r,
            Route::ClearSelection,
            "聚焦态双击空白应产生 ClearSelection"
        );
        assert_ne!(
            r,
            Route::ExitNodeFocus,
            "双击空白不是显式退出聚焦，不应产生 ExitNodeFocus"
        );
    }

    /// 点击节点标题栏的关闭按钮 → 路由为 RemoveNode，而不是普通选中或清空。
    ///
    /// 数据构造（含关键数值推导）：
    ///   close target node_id = 7（来自 UI 渲染时捕获的卡片 id）
    ///   当前模式分别覆盖 CanvasSelect 与 NodeFocused(7)
    ///
    /// 执行过程：
    ///   1. CanvasSelect + CloseNode(7) → route
    ///   2. NodeFocused(7) + CloseNode(7) → route
    ///   3. 对结果做正负断言
    ///
    /// 预期结果：
    ///   - 正断言：两个模式下都返回 RemoveNode(7)
    ///   - 负断言：关闭按钮不应退化为 SelectNode 或 ClearSelection
    #[test]
    fn test_close_node_routes_to_remove_node() {
        let from_canvas = route(&InputMode::CanvasSelect, &InputEvent::CloseNode { node: 7 });
        let from_focus = route(
            &InputMode::NodeFocused(7),
            &InputEvent::CloseNode { node: 7 },
        );

        assert_eq!(
            from_canvas,
            Route::RemoveNode(7),
            "画布模式点击关闭按钮应路由为 RemoveNode(7)"
        );
        assert_eq!(
            from_focus,
            Route::RemoveNode(7),
            "节点聚焦模式点击自身关闭按钮也应路由为 RemoveNode(7)"
        );
        assert_ne!(
            from_canvas,
            Route::SelectNode(7),
            "关闭按钮不应只选中节点，否则卡片不会被删除"
        );
        assert_ne!(
            from_focus,
            Route::ClearSelection,
            "关闭按钮不应只清空选择，否则 live node 会泄漏"
        );
    }

    /// Master layout 快捷键：未激活且有目标时进入，已激活时退出，无目标时忽略。
    ///
    /// 数据构造（含关键数值推导）：
    ///   target node id = 8（来自 hover 优先、selection 兜底的 app 层解析）
    ///   inactive event = ToggleMasterLayout { active=false, target=Some(8) }
    ///   active event   = ToggleMasterLayout { active=true, target=Some(8) }
    ///   missing event  = ToggleMasterLayout { active=false, target=None }
    ///
    /// 执行过程：
    ///   1. inactive event → route(CanvasSelect)
    ///   2. active event → route(CanvasSelect)
    ///   3. missing event → route(CanvasSelect)
    ///
    /// 预期结果：
    ///   - 正断言：inactive + target 返回 EnterMasterLayout(8)
    ///   - 正断言：active 返回 ExitMasterLayout
    ///   - 正断言：inactive + None 返回 Ignore
    ///   - 负断言：active 不应再次返回 EnterMasterLayout(8)
    #[test]
    fn test_toggle_master_layout_routes_enter_exit_and_ignore() {
        let enter = route(
            &InputMode::CanvasSelect,
            &InputEvent::ToggleMasterLayout {
                active: false,
                target: Some(8),
            },
        );
        let exit = route(
            &InputMode::CanvasSelect,
            &InputEvent::ToggleMasterLayout {
                active: true,
                target: Some(8),
            },
        );
        let missing = route(
            &InputMode::CanvasSelect,
            &InputEvent::ToggleMasterLayout {
                active: false,
                target: None,
            },
        );

        assert_eq!(
            enter,
            Route::EnterMasterLayout(8),
            "未激活且有 target=8 时应进入 master layout"
        );
        assert_eq!(
            exit,
            Route::ExitMasterLayout,
            "已激活时再次按快捷键应退出 master layout"
        );
        assert_eq!(
            missing,
            Route::Ignore,
            "未激活但无 hover/selection 目标时应忽略快捷键"
        );
        assert_ne!(
            exit,
            Route::EnterMasterLayout(8),
            "已激活时不应再次进入同一个 master layout"
        );
    }
}
