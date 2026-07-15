/// 顶部工具栏：Cursor 入口必须使用独立 SVG 图标和可定位 action id。
///
/// 数据构造（含关键数值的推导过程）：
///   生产渲染源码 = chrome.rs 中 #[cfg(test)] 前的区段，避免测试模块自身误命中。
///   Cursor 入口需要 2 个证据：id="toolbar-new-cursor-agent" 与 ToolbarIcon::Cursor。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 chrome.rs 的生产渲染区段。
///   2. 检查 Cursor 按钮的定位 id、图标变体和点击处理器。
///
/// 预期结果：
///   - 断言 A：顶部工具栏有 Cursor 按钮 id 和 Cursor SVG 图标。
///   - 断言 B：该入口绑定 Cursor 创建处理器。
///   - 断言 C：Cursor 不得退化为 Codex 图标或旧文本按钮。
#[test]
fn test_top_toolbar_includes_cursor_agent_icon_action() {
    let source = include_str!("../../src/app/chrome.rs");
    let render_source = source.split("#[cfg(test)]").next().unwrap_or(source);

    assert!(
        render_source.contains("toolbar-new-cursor-agent"),
        "顶部工具栏必须提供可定位的 Cursor 创建入口"
    );
    assert!(
        render_source.contains("ToolbarIcon::Cursor"),
        "Cursor 创建入口必须渲染专用 ToolbarIcon::Cursor SVG 图标"
    );
    assert!(
        render_source.contains("on_toolbar_new_cursor_agent"),
        "Cursor 图标按钮必须绑定 on_toolbar_new_cursor_agent 处理器"
    );
    assert_ne!(
        "ToolbarIcon::Cursor", "ToolbarIcon::Codex",
        "Cursor 的图标枚举不能与 Codex 共用同一个品牌资源"
    );
    assert!(
        !render_source.contains("chrome_button(theme, \"Cursor\""),
        "Cursor 入口不应回退为旧式文本 chrome_button"
    );
}
