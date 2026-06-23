use super::*;
use canvas::{CanvasNodeKind, CanvasRect, RectNode};
use gpui::{Keystroke, Modifiers};
use runtime::{InputMode, Runtime};

/// active selection 复制：没有文本选区时 Cmd+C 的文本来源应为空。
///
/// 数据构造（含关键数值的推导过程）：
///   runtime = Runtime::new()，没有 live terminal view
///   active_text_selection = None，表示用户没有拖选任何文本
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用 copy_text_for_active_selection(runtime, None)
///   2. 读取返回值
///
/// 预期结果：
///   - 正断言：返回 None，调用方不会写剪贴板
///   - 负断言：不应退化为 Some("") 或整卡 fallback
#[test]
fn test_copy_text_for_active_selection_returns_none_without_selection() {
    let runtime = Runtime::new();

    let text = copy_text_for_active_selection(&runtime, None);

    assert_eq!(
        text, None,
        "没有 active text selection 时 Cmd+C 不应产生可写入剪贴板的文本"
    );
    assert_ne!(
        text,
        Some(String::new()),
        "无选区不应返回空字符串 Some，否则调用方可能误以为复制成功"
    );
}

/// active selection 复制：非 terminal/agent 的选区不应触发旧整卡复制 fallback。
///
/// 数据构造（含关键数值的推导过程）：
///   rect node_id = 1，普通 Rect 没有 runtime live node view
///   selection anchor=(0,0), head=(0,4)，模拟误把非文本卡片放进 active selection
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 Runtime document 中创建 Rect node
///   2. 构造 TextSelection(node_id=rect, range=0..4)
///   3. 调用 copy_text_for_active_selection
///
/// 预期结果：
///   - 正断言：返回 None，非 terminal view 不复制
///   - 负断言：不应返回 Rect/Preview 的整卡文本 fallback
#[test]
fn test_copy_text_for_active_selection_ignores_non_terminal_nodes() {
    let mut runtime = Runtime::new();
    let rect = runtime.document.add_node(
        CanvasNodeKind::Rect(RectNode { color: 0x4080ff }),
        CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );
    let selection = crate::text_selection::TextSelection::new(
        rect,
        crate::text_selection::TextPoint::new(0, 0),
    )
    .with_head(crate::text_selection::TextPoint::new(0, 4));

    let text = copy_text_for_active_selection(&runtime, Some(selection));

    assert_eq!(
        text, None,
        "active selection 指向非 terminal live node 时不应复制任何内容"
    );
    assert_ne!(
        text,
        Some("Rect".to_string()),
        "非 terminal 选区不应走旧的整卡/预览文本 fallback"
    );
}

/// 当前卡片复制目标：节点聚焦优先于画布选择，避免 Cmd+C 复制到旧选中卡片。
///
/// 数据构造（含关键数值的推导过程）：
///   A node_id = 1（Document 首个节点）
///   B node_id = 2（Document 第二个节点）
///   selection = {1}
///   focus = NodeFocused(2)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 A/B 两张普通卡片
///   2. selection.select_only(A) → 画布仍选中 A
///   3. focus.focus_node(B) → 用户当前正在 B 卡片内操作
///   4. current_copy_node_id(runtime) → 应选择当前聚焦的 B
///
/// 预期结果：
///   - 正断言：返回 Some(B)，Cmd+C 的复制来源是当前聚焦卡片
///   - 负断言：不返回 Some(A)，避免复制旧 selection 内容
#[test]
fn test_current_copy_node_id_prefers_focused_card_over_selection() {
    let mut runtime = Runtime::new();
    let a = runtime.document.add_node(
        CanvasNodeKind::Rect(RectNode { color: 0x4080ff }),
        CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );
    let b = runtime.document.add_node(
        CanvasNodeKind::Rect(RectNode { color: 0xff8040 }),
        CanvasRect::new(120.0, 0.0, 100.0, 80.0),
    );
    runtime.selection.select_only(a);
    runtime.focus.focus_node(b);

    let target = current_copy_node_id(&runtime);

    assert_eq!(
        target,
        Some(b),
        "当前复制目标应优先使用聚焦卡片 B，而不是旧 selection"
    );
    assert_ne!(
        target,
        Some(a),
        "当前复制目标不应退回旧选中的 A，否则用户在 B 中 Cmd+C 会复制错卡片"
    );
}

/// Terminal 可见文本复制：复制内容应去掉每行右侧填充空格和末尾空白行。
///
/// 数据构造（含关键数值的推导过程）：
///   rows = 3, cols = 6
///   row0 = "COPY  "（4 个可见字符 + 2 个 terminal 填充空格）
///   row1 = "OK    "（2 个可见字符 + 4 个 terminal 填充空格）
///   row2 = "      "（整行空白，属于 terminal 可视区域填充）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 GridSnapshot，模拟 terminal renderer 当前可见区域
///   2. terminal_snapshot_copy_text(snapshot)
///   3. 对比复制文本与原始 terminal cell 矩阵
///
/// 预期结果：
///   - 正断言：复制文本为 "COPY\nOK"
///   - 负断言：不保留右侧填充空格
///   - 负断言：不保留末尾空白行
#[test]
fn test_terminal_snapshot_copy_text_trims_padding_and_blank_tail() {
    use terminal::{Cell, CellAttrs, GridSnapshot, TerminalColor, TerminalNamedColor};

    fn cell(c: char) -> Cell {
        Cell {
            c,
            fg: TerminalColor::Named(TerminalNamedColor::Foreground),
            bg: TerminalColor::Named(TerminalNamedColor::Background),
            attrs: CellAttrs::default(),
        }
    }

    let snapshot = GridSnapshot {
        rows: vec![
            "COPY  ".chars().map(cell).collect(),
            "OK    ".chars().map(cell).collect(),
            "      ".chars().map(cell).collect(),
        ],
        cursor_line: 1,
        cursor_col: 2,
        cols: 6,
        lines: 3,
        display_offset: 0,
        history_size: 0,
    };

    let text = terminal_snapshot_copy_text(&snapshot);

    assert_eq!(
        text, "COPY\nOK",
        "复制 terminal 内容时应保留可见文本行，并裁掉 terminal 填充"
    );
    assert_ne!(text, "COPY  \nOK    ", "复制文本不应包含每行右侧填充空格");
    assert!(
        !text.ends_with('\n'),
        "复制文本不应保留末尾空白行产生的尾随换行"
    );
}

/// 方向键识别：只有裸方向键进入画布卡片导航，带修饰键时不抢系统/终端快捷键。
///
/// 数据构造（含关键数值的推导过程）：
///   key="right"，modifiers=default() → 裸方向键
///   key="right"，modifiers.shift=true → 带 Shift 的方向键
///   key="a"，modifiers=default() → 普通字符键
///
/// 执行过程（逐步说明系统如何处理）：
///   1. canvas_navigation_direction("right", default) → Some(Right)
///   2. canvas_navigation_direction("right", shift) → None
///   3. canvas_navigation_direction("a", default) → None
///
/// 预期结果：
///   - 正断言：裸 right 映射为 NavigationDirection::Right
///   - 负断言：Shift+right 不映射，避免覆盖组合快捷键
///   - 负断言：普通字符键不映射，避免误触发卡片选择
#[test]
fn test_canvas_navigation_direction_only_accepts_unmodified_arrow_keys() {
    let bare_right = canvas_navigation_direction("right", Modifiers::default());
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    let shifted_right = canvas_navigation_direction("right", shift);
    let plain_char = canvas_navigation_direction("a", Modifiers::default());

    assert_eq!(
        bare_right,
        Some(canvas::NavigationDirection::Right),
        "裸 right 应映射为画布向右导航"
    );
    assert_eq!(
        shifted_right, None,
        "Shift+right 不应触发画布导航，避免吞掉组合快捷键"
    );
    assert_eq!(
        plain_char, None,
        "普通字符键不应触发画布导航，否则会干扰文本输入"
    );
}

/// 当前选择目标解析：只有单选且节点仍存在时，才作为方向导航的起点。
///
/// 数据构造（含关键数值的推导过程）：
///   A node_id=1，B node_id=2
///   single selection={1}
///   multi selection={1,2}
///   stale selection={99}，但 document 中不存在 id=99
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 单选 A → current_selection_node_id 返回 Some(A)
///   2. 多选 A+B → 返回 None，让导航回退视口中心
///   3. 旧选择 99 → 返回 None，让导航回退视口中心
///
/// 预期结果：
///   - 正断言：单选存在节点时返回 Some(A)
///   - 负断言：多选不返回任一节点
///   - 负断言：失效选择不返回不存在的 id=99
#[test]
fn test_current_selection_node_id_requires_single_existing_selection() {
    let mut runtime = Runtime::new();
    let a = runtime.document.add_node(
        CanvasNodeKind::Rect(RectNode { color: 0x4080ff }),
        CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );
    let b = runtime.document.add_node(
        CanvasNodeKind::Rect(RectNode { color: 0xff8040 }),
        CanvasRect::new(140.0, 0.0, 100.0, 80.0),
    );

    runtime.selection.select_only(a);
    let single = current_selection_node_id(&runtime);

    runtime.selection.add(b);
    let multi = current_selection_node_id(&runtime);

    runtime.selection.clear();
    runtime.selection.select_only(99);
    let stale = current_selection_node_id(&runtime);

    assert_eq!(
        single,
        Some(a),
        "单选且节点存在时应返回该节点作为方向导航起点"
    );
    assert_eq!(multi, None, "多选时不应任选一个节点，否则导航起点不明确");
    assert_ne!(multi, Some(a), "多选时不应悄悄选 A 作为导航起点");
    assert_eq!(
        stale, None,
        "旧选择 id=99 不存在时应返回 None 并回退视口中心"
    );
    assert_ne!(stale, Some(99), "失效选择不应返回不存在的 id=99");
}

/// 方向键选中的可聚焦卡片：后续内容输入前应能解析为自动聚焦目标。
///
/// 数据构造（含关键数值的推导过程）：
///   terminal id=1，spawn_terminal 会注册 live node，因此 node_caps(1).focusable=true
///   rect id=2，普通 Rect 没有 live node，因此 node_caps(2).focusable=false
///   keyboard_selected_node=Some(terminal) 表示刚由方向键选中 terminal
///
/// 执行过程（逐步说明系统如何处理）：
///   1. selection={terminal} 且 keyboard_selected_node=terminal → 解析自动聚焦目标
///   2. selection={rect} 且 keyboard_selected_node=rect → 解析自动聚焦目标
///   3. selection={terminal} 但 keyboard_selected_node=None → 解析自动聚焦目标
///
/// 预期结果：
///   - 正断言：方向键选中的 terminal 返回 Some(terminal)
///   - 负断言：普通 rect 不返回目标，避免把文本输入塞给不可输入卡片
///   - 负断言：不是方向键来源的选择不返回目标，避免鼠标标题栏选择后自动聚焦
#[test]
fn test_keyboard_selected_focus_target_requires_keyboard_selected_focusable_card() {
    let mut runtime = Runtime::new();
    let terminal = runtime
        .spawn_terminal(CanvasRect::new(0.0, 0.0, 100.0, 80.0), "/bin/sh", 20, 4)
        .expect("测试 terminal 应能启动 /bin/sh");
    let rect = runtime.document.add_node(
        CanvasNodeKind::Rect(RectNode { color: 0xff8040 }),
        CanvasRect::new(140.0, 0.0, 100.0, 80.0),
    );

    runtime.selection.select_only(terminal);
    let terminal_target = keyboard_selected_focus_target(&runtime, Some(terminal));

    runtime.selection.select_only(rect);
    let rect_target = keyboard_selected_focus_target(&runtime, Some(rect));

    runtime.selection.select_only(terminal);
    let mouse_selected_target = keyboard_selected_focus_target(&runtime, None);
    runtime.kill_all();

    assert_eq!(
        terminal_target,
        Some(terminal),
        "方向键选中的可聚焦 terminal 应成为后续输入的自动聚焦目标"
    );
    assert_eq!(
        rect_target, None,
        "方向键选中的普通 Rect 不可聚焦，不应成为输入目标"
    );
    assert_ne!(
        rect_target,
        Some(rect),
        "不可输入 Rect 不应接收后续键盘内容"
    );
    assert_eq!(
        mouse_selected_target, None,
        "非方向键来源的选择不应自动聚焦，避免鼠标标题栏选择后输入冲突"
    );
    assert_eq!(
        runtime.focus.mode(),
        InputMode::CanvasSelect,
        "解析自动聚焦目标不应自己改变 focus，实际聚焦由 keydown 执行"
    );
}

/// 自动聚焦触发键：内容输入会触发，方向键、空格拖拽和平台快捷键不会触发。
///
/// 数据构造（含关键数值的推导过程）：
///   printable: key="a", key_char=Some("a") → IME/文本输入内容
///   enter: key="enter" → terminal 控制输入内容
///   arrow: key="right" → 已由画布导航使用
///   space: key="space" → CanvasSelect 下保留给空格拖拽
///   cmd_c: platform=true,key="c" → app 复制快捷键
///   cmd_v: platform=true,key="v" → paste 内容输入
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别调用 should_auto_focus_keyboard_selection
///   2. 比较内容键与交互/快捷键的返回值
///
/// 预期结果：
///   - 正断言：printable/enter/cmd_v 返回 true
///   - 负断言：arrow/space/cmd_c 返回 false
#[test]
fn test_should_auto_focus_keyboard_selection_accepts_content_but_not_navigation_or_shortcuts() {
    fn key(key: &str, key_char: Option<&str>, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.to_string(),
            key_char: key_char.map(str::to_string),
        }
    }

    let printable = key("a", Some("a"), Modifiers::default());
    let enter = key("enter", None, Modifiers::default());
    let arrow = key("right", None, Modifiers::default());
    let space = key("space", Some(" "), Modifiers::default());
    let platform = Modifiers {
        platform: true,
        ..Modifiers::default()
    };
    let cmd_c = key("c", None, platform);
    let cmd_v = key("v", None, platform);

    assert!(
        should_auto_focus_keyboard_selection(&printable),
        "方向键选中 terminal 后输入普通字符应触发自动聚焦"
    );
    assert!(
        should_auto_focus_keyboard_selection(&enter),
        "方向键选中 terminal 后按 Enter 应触发自动聚焦并发送 Enter"
    );
    assert!(
        should_auto_focus_keyboard_selection(&cmd_v),
        "方向键选中 terminal 后 Cmd+V 是内容粘贴，应触发自动聚焦"
    );
    assert!(
        !should_auto_focus_keyboard_selection(&arrow),
        "裸方向键应继续用于画布导航，不应转成输入聚焦"
    );
    assert!(
        !should_auto_focus_keyboard_selection(&space),
        "裸 Space 在 CanvasSelect 下保留给空格拖拽，不应触发自动聚焦"
    );
    assert!(
        !should_auto_focus_keyboard_selection(&cmd_c),
        "Cmd+C 应保留给复制动作，不应自动聚焦 terminal"
    );
}
