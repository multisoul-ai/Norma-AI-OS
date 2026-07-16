use super::*;

/// B-14: Settings 覆盖画布时必须隐藏原生 Browser view，关闭后再恢复。
///
/// 数据构造（含关键数值的推导过程）：
///   - settings_open = true：Settings 作为 1 个全画布浮层出现，原生 child view 不能参与显示。
///   - settings_open = false：浮层数量 = 0，画布中的 Browser view 应继续正常显示。
///   - canvas render = 1 次：同一帧先计算显示许可，再决定是否同步 Browser 节点。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别把 Settings 打开和关闭状态交给原生 Browser 显示决策。
///   2. 打开时，画布跳过 Browser 原生视图同步，最终用空集合隐藏已有视图。
///   3. 关闭时，画布重新收集可见 Browser 节点并同步原生视图。
///
/// 预期结果：
///   - 正断言：Settings 关闭时允许原生 Browser view 显示。
///   - 负断言：Settings 打开时不允许原生 Browser view 穿透浮层。
///   - 正断言：画布真实同步调用点使用该决策，而不是仅有未接线的纯函数。
#[test]
fn test_settings_overlay_hides_native_browser_views_until_closed() {
    let canvas_source = include_str!("../../src/canvas/view.rs");

    assert!(
        native_browser_views_visible(false),
        "Settings 关闭时 Browser 原生内容必须恢复显示，不能留下空白卡片"
    );
    assert!(
        !native_browser_views_visible(true),
        "Settings 打开时 Browser 原生内容必须隐藏，否则 macOS child view 会穿透 GPUI 浮层"
    );
    assert!(
        canvas_source.contains(
            "let native_browser_views_visible = native_browser_views_visible(app.settings_panel.open);"
        ),
        "render_canvas 必须读取 Settings 状态并计算原生 Browser 显示许可"
    );
    assert!(
        canvas_source.contains("if native_browser_views_visible {"),
        "Browser 原生视图同步必须受 Settings 显示许可门控，不能继续无条件 set_visible(true)"
    );
}
