//! 焦点状态机：画布选择 ↔ 终端聚焦。

use canvas::NodeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// 画布选择模式：点击选中/拖拽节点、pan/zoom。
    CanvasSelect,
    /// 节点聚焦模式：键盘输入发送到指定 live node。
    NodeFocused(NodeId),
}

#[derive(Debug, Clone)]
pub struct FocusState {
    mode: InputMode,
}

impl Default for FocusState {
    fn default() -> Self {
        FocusState {
            mode: InputMode::CanvasSelect,
        }
    }
}

impl FocusState {
    pub fn new() -> Self {
        FocusState::default()
    }

    pub fn mode(&self) -> InputMode {
        self.mode
    }

    pub fn focus_node(&mut self, node: NodeId) {
        self.mode = InputMode::NodeFocused(node);
    }

    pub fn exit_to_canvas(&mut self) {
        self.mode = InputMode::CanvasSelect;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FocusState 初始模式为 CanvasSelect；focus_node 切换到 NodeFocused；
    /// exit_to_canvas 退回 CanvasSelect。
    ///
    /// 数据构造：
    ///   node_id = 42（任意非零 id，用于标识目标终端节点）
    ///
    /// 执行过程：
    ///   1. FocusState::new() → mode = CanvasSelect
    ///   2. focus_node(42) → mode = NodeFocused(42)
    ///   3. exit_to_canvas() → mode = CanvasSelect
    ///
    /// 预期结果：
    ///   - 正断言：new() 后 mode == CanvasSelect
    ///   - 正断言：focus_node(42) 后 mode == NodeFocused(42)
    ///   - 负断言：focus_node(42) 后 mode != CanvasSelect
    ///   - 正断言：exit_to_canvas() 后 mode == CanvasSelect
    ///   - 负断言：exit_to_canvas() 后 mode != NodeFocused(42)
    #[test]
    fn test_focus_state_transitions() {
        let mut f = FocusState::new();
        assert_eq!(
            f.mode(),
            InputMode::CanvasSelect,
            "新建 FocusState 应为 CanvasSelect 模式"
        );

        f.focus_node(42);
        assert_eq!(
            f.mode(),
            InputMode::NodeFocused(42),
            "focus_node(42) 后应为 NodeFocused(42)"
        );
        assert_ne!(
            f.mode(),
            InputMode::CanvasSelect,
            "聚焦终端后不应仍在 CanvasSelect 模式"
        );

        f.exit_to_canvas();
        assert_eq!(
            f.mode(),
            InputMode::CanvasSelect,
            "exit_to_canvas() 后应回到 CanvasSelect"
        );
        assert_ne!(
            f.mode(),
            InputMode::NodeFocused(42),
            "退出聚焦后不应仍为 NodeFocused(42)"
        );
    }

    /// 连续切换不同节点：最后一次 focus_node 覆盖前一次。
    ///
    /// 数据构造：
    ///   先聚焦 node_id=1，再聚焦 node_id=7
    ///
    /// 执行过程：
    ///   1. focus_node(1) → mode = NodeFocused(1)
    ///   2. focus_node(7) → mode = NodeFocused(7)（覆盖）
    ///
    /// 预期结果：
    ///   - 正断言：mode == NodeFocused(7)（最新赋值生效）
    ///   - 负断言：mode != NodeFocused(1)（旧节点已被覆盖）
    #[test]
    fn test_focus_node_overrides_previous() {
        let mut f = FocusState::new();
        f.focus_node(1);
        f.focus_node(7);
        assert_eq!(
            f.mode(),
            InputMode::NodeFocused(7),
            "第二次 focus_node 应覆盖第一次，最终聚焦节点 7"
        );
        assert_ne!(
            f.mode(),
            InputMode::NodeFocused(1),
            "旧的聚焦节点 1 已被覆盖，不应出现在 mode 中"
        );
    }
}
