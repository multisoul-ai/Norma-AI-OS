use super::*;
use crate::canvas::text_selection::TextPoint;

/// 文本选区拖动性能：鼠标仍在同一 cell 时不应报告状态变化。
///
/// 数据构造（含关键数值推导）：
///   node_id = 7
///   anchor = head = row 1, col 2
///   第二次 head 仍为 row 1, col 2，因此 row/col 差值均为 0
///
/// 执行过程：
///   1. begin_text_selection(node=7, point=(1,2))
///   2. update_text_selection_head(node=7, point=(1,2))
///   3. 读取 changed 与 active_text_selection
///
/// 预期结果：
///   - 正断言：changed=false，调用方不需要 cx.notify()
///   - 负断言：head 不应被改成其他坐标
#[test]
fn test_text_selection_head_update_ignores_same_cell_for_notify_throttling() {
    let mut state = InteractionState::default();
    state.begin_text_selection(7, TextPoint::new(1, 2));

    let changed = state.update_text_selection_head(7, TextPoint::new(1, 2));
    let selection = state
        .active_text_selection
        .expect("begin_text_selection 应创建 active selection");

    assert!(
        !changed,
        "鼠标仍在同一 row/col 时应返回 false，避免重复 notify 和重绘"
    );
    assert_eq!(
        selection.head,
        TextPoint::new(1, 2),
        "同 cell 更新后 head 应保持原坐标"
    );
    assert_ne!(
        selection.head,
        TextPoint::new(1, 3),
        "同 cell 移动不应产生新的列坐标"
    );
}

/// 文本选区拖动更新：跨到新 cell 时应报告状态变化并移动 head。
///
/// 数据构造（含关键数值推导）：
///   node_id = 7
///   anchor/head 初始 = row 1, col 2
///   新 head = row 1, col 5，因此 col delta = 5 - 2 = 3
///
/// 执行过程：
///   1. begin_text_selection(node=7, point=(1,2))
///   2. update_text_selection_head(node=7, point=(1,5))
///   3. 读取 changed 与 head
///
/// 预期结果：
///   - 正断言：changed=true 且 head 更新为 (1,5)
///   - 负断言：head 不应停在旧坐标 (1,2)
#[test]
fn test_text_selection_head_update_changes_when_cell_changes() {
    let mut state = InteractionState::default();
    state.begin_text_selection(7, TextPoint::new(1, 2));

    let changed = state.update_text_selection_head(7, TextPoint::new(1, 5));
    let selection = state
        .active_text_selection
        .expect("begin_text_selection 应创建 active selection");

    assert!(
        changed,
        "鼠标进入新的 row/col 时应返回 true，让调用方触发重绘"
    );
    assert_eq!(
        selection.head,
        TextPoint::new(1, 5),
        "head 应移动到新的 cell 坐标"
    );
    assert_ne!(
        selection.head,
        TextPoint::new(1, 2),
        "跨 cell 更新后 head 不应停在旧坐标"
    );
}

/// 文本选区完成：没有拖出宽度的单击应清除临时选区。
///
/// 数据构造（含关键数值推导）：
///   node_id = 7
///   anchor = head = row 0, col 0
///   range 宽度 = 0，因此不是有效文本选择
///
/// 执行过程：
///   1. begin_text_selection(node=7, point=(0,0))
///   2. finish_text_selection()
///   3. 检查 active_text_selection
///
/// 预期结果：
///   - 正断言：active_text_selection=None
///   - 负断言：单击正文不应留下空选区
#[test]
fn test_finish_text_selection_clears_empty_click_selection() {
    let mut state = InteractionState::default();
    state.begin_text_selection(7, TextPoint::new(0, 0));

    state.finish_text_selection();

    assert_eq!(
        state.active_text_selection, None,
        "没有拖出宽度的正文单击应清除临时文本选区"
    );
    assert!(
        state.active_text_selection.is_none(),
        "空选区不应保留，否则 Cmd+C 会看到 stale selection"
    );
}

/// ✚ 弹出菜单状态：toggle 翻转 open 标志，close 强制收起。
///
/// 数据构造（含关键数值推导）：
///   state = InteractionState::default()
///   new_menu_open 初值 = false（Default 派生，bool 默认 false）
///   第一次 toggle_new_menu() → !false = true
///   第二次 toggle_new_menu() → !true = false
///   再 toggle 打开 → true，随后 close_new_menu() → false
///
/// 执行过程（逐步说明系统如何处理）：
///   1. fresh default → new_menu_open=false（菜单默认关闭）
///   2. toggle_new_menu() → new_menu_open=true（✚ 按钮点击展开菜单）
///   3. toggle_new_menu() → new_menu_open=false（再次点击 ✚ 收起菜单）
///   4. toggle_new_menu() 后 close_new_menu() → new_menu_open=false（任意收起路径）
///
/// 预期结果：
///   - 正断言：toggle 后应展开（true）；close 后应为 false
///   - 负断言：初始状态不应为 true；二次 toggle 后不应保持 true
#[test]
fn test_new_menu_state_toggles_and_closes() {
    let mut state = InteractionState::default();

    assert!(
        !state.new_menu_open,
        "InteractionState 默认 new_menu_open 应为 false，菜单初始关闭"
    );

    state.toggle_new_menu();
    assert!(
        state.new_menu_open,
        "第一次 toggle_new_menu 后菜单应展开（new_menu_open=true）"
    );

    state.toggle_new_menu();
    assert!(
        !state.new_menu_open,
        "第二次 toggle_new_menu 后菜单应收起（new_menu_open=false）"
    );

    state.toggle_new_menu();
    assert!(
        state.new_menu_open,
        "再次 toggle 后菜单应展开，准备验证 close_new_menu"
    );

    state.close_new_menu();
    assert!(
        !state.new_menu_open,
        "close_new_menu 应无条件收起菜单（new_menu_open=false）"
    );
}
