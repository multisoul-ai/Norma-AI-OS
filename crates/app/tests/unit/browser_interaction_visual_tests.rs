/// B-14：Browser 地址栏聚焦后必须能通过鼠标落点折叠全选，并持续绘制可见插入光标。
///
/// 数据构造（含关键数值的推导过程）：
///   - 初始 URL 长度 = N bytes；进入编辑态时选区为 `0..N`，用于保留浏览器地址栏全选体验。
///   - 第二次左键点击坐标 = `event.position.x`；文字布局将该坐标映射到 `0..=N` 的 UTF-8 边界。
///   - caret 宽度至少为 2px；即使选区非空，也要在 active edge 绘制，避免用户只看到一层淡选区。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 地址栏已聚焦时接收第二次 MouseDownEvent。
///   2. 输入实体通过最后一次文字布局把点击坐标换算为字符边界，并把选区折叠到该位置。
///   3. 渲染层独立计算 selection 与 caret；聚焦时先画选区，再画文字和 caret。
///
/// 预期结果：
///   - 正断言：鼠标落点参与插入位置计算，第二次点击不再维持全选。
///   - 正断言：I-beam 鼠标指针和至少 2px 的 caret 都存在。
///   - 负断言：caret 不得与 selection 互斥，否则初次聚焦仍会表现为“没有光标”。
#[test]
fn test_browser_address_click_places_a_visible_caret_even_with_selection() {
    let input_source = include_str!("../../src/app/browser/address_input.rs");
    let render_source = include_str!("../../src/app/browser/address_input/render.rs");

    assert!(
        input_source.contains("event: &MouseDownEvent"),
        "地址栏 MouseDown 必须读取真实事件，才能按点击坐标放置插入光标"
    );
    assert!(
        input_source.contains("event.position") && input_source.contains("closest_index_for_x"),
        "地址栏已聚焦时必须把鼠标 x 坐标映射到最近字符边界"
    );
    assert!(
        render_source.contains(".cursor(CursorStyle::IBeam)"),
        "地址栏 hover 时必须显示系统 I-beam，明确这里是可编辑文本"
    );
    assert!(
        render_source.contains("size(px(2.0), bounds.size.height)"),
        "地址栏 caret 必须至少 2px 宽，避免在高分屏和半透明背景上难以辨认"
    );
    assert!(
        !render_source.contains("let (selection, cursor) = if input.selected_range.is_empty()"),
        "selection 与 caret 不得互斥；初次聚焦全选 URL 时仍应看到 active-edge caret"
    );
}

/// B-15：Browser 工具栏按钮必须提供静止、悬停和按下三段视觉反馈。
///
/// 数据构造（含关键数值的推导过程）：
///   - 按钮数量 = 3（Back、Forward、Reload），三者复用同一个 `toolbar_button` builder。
///   - 交互状态 = rest、hover、active 共 3 段；只要 builder 覆盖一次，三个按钮都会获得一致反馈。
///   - active 使用 accent_cyan；与普通 edge_highlight 区分，按下时形成明确“选中”效果。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 `toolbar_button` 的生产渲染区。
///   2. 检查按钮是否声明 pointer cursor 和稳定默认底色/边框。
///   3. 检查 hover 与 active 是否分别覆盖背景和边框颜色。
///
/// 预期结果：
///   - 正断言：按钮 hover 时有轻量反馈，active 时使用强调色反馈。
///   - 正断言：鼠标进入按钮时显示 pointer cursor。
///   - 负断言：不能只绑定 mouse up 而完全没有交互样式。
#[test]
fn test_browser_toolbar_buttons_expose_hover_and_pressed_feedback() {
    let source = include_str!("../../src/cards/browser/card.rs");
    let start = source
        .find("fn toolbar_button(")
        .expect("Browser card 必须保留统一的 toolbar_button builder");
    let end = source[start..]
        .find("fn browser_url(")
        .map(|offset| start + offset)
        .expect("toolbar_button 应位于 browser_url helper 之前");
    let button_block = &source[start..end];

    assert!(
        button_block.contains(".cursor_pointer()"),
        "Browser 工具栏按钮必须显示 pointer cursor，明确该区域可点击"
    );
    assert!(
        button_block.contains(".hover("),
        "Browser 工具栏按钮必须提供 hover 背景或边框反馈"
    );
    assert!(
        button_block.contains(".active("),
        "Browser 工具栏按钮必须提供 mouse-down active 反馈，不能点击后毫无选中效果"
    );
    assert!(
        button_block.contains("theme.accent_cyan"),
        "Browser 工具栏 active 状态必须使用主题强调色，与静止状态形成清晰区分"
    );
    assert!(
        !button_block.contains(".child(label)\n        .on_mouse_down"),
        "按钮不能从纯文字直接跳到事件绑定；中间必须存在可见交互样式"
    );
}
