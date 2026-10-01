use super::*;

/// 紧凑卡片保持 32px 标题高度，内层圆角随边框宽度收缩且与外框同心。
///
/// 数据构造（含关键数值推导）：
///   zoom=1 → 外圆角 14px；普通边框 1px → 内圆角 14-1=13px。
///   聚焦边框 2px → 内圆角 14-2=12px；标题高 32px → GPUI 圆角上限 16px。
///   zoom=0.125 → 外圆角 14×0.125=1.75px，小于 2px 边框，内圆角应归零。
///
/// 执行过程：
///   1. 计算正常/聚焦状态的内圈半径。
///   2. 将聚焦标题栏交给 GPUI 实际圆角限制逻辑。
///   3. 计算极小缩放下的内圈半径。
///
/// 预期结果：
///   - 正断言：外圈14、普通内圈13、聚焦内圈12、极小内圈0，标题高32px。
///   - 正断言：GPUI 绘制的左右顶角均保持12px。
///   - 负断言：内圈不得直接复用外圈14px，否则圆心错位。
#[test]
fn test_compact_card_content_radii_follow_border_insets() {
    let radius = node_card_radius_px(1.0);
    let normal = node_content_radius_px(1.0, 1.0);
    let focused = node_content_radius_px(1.0, 2.0);
    // 320px 是测试卡片宽度；半径上限由较短的 32px 标题高度决定。
    let painted = gpui::Corners::all(px(focused))
        .clamp_radii_for_quad_size(gpui::size(px(320.0), px(TITLE_BAR_H)));

    assert_eq!(radius, 14.0, "外壳应使用紧凑的14px圆角");
    assert_eq!(normal, 13.0, "普通内圈应扣除1px边框");
    assert_eq!(focused, 12.0, "聚焦内圈应扣除2px边框");
    assert_eq!(TITLE_BAR_H, 32.0, "圆角调整不得挤压标题和按钮高度");
    assert_eq!(painted.top_left, px(12.0), "左上内弧不应被GPUI二次缩小");
    assert_eq!(painted.top_right, px(12.0), "右上内弧不应被GPUI二次缩小");
    assert_eq!(
        node_content_radius_px(0.125, 2.0),
        0.0,
        "极小卡片的内圆角不得为负"
    );
    assert_ne!(normal, radius, "普通内圈不得复用外圈半径");
    assert_ne!(focused, radius, "聚焦内圈不得复用外圈半径");
}

/// Product UI V2 标题栏类型徽章：五种节点必须映射为稳定的 uppercase 类型标签。
///
/// 数据构造（含关键数值推导）：
///   NodeType 变体 = 5（Term/Agent/Preview/Browser/Rect）
///   目标标签 = TERMINAL/AGENT/PREVIEW/BROWSER/NODE
///
/// 执行过程：
///   1. 对五种 NodeType 调用 node_type_label。
///   2. 逐项检查标签，确保标题栏视觉不会依赖可变业务标题。
///
/// 预期结果：
///   - 正断言：五种类型均映射到稳定 uppercase 标签。
///   - 负断言：Terminal 类型标签不得退化为混合大小写业务标题。
#[test]
fn test_product_ui_v2_titlebar_uses_stable_uppercase_type_badges() {
    assert_eq!(
        node_type_label(NodeType::Term),
        "TERMINAL",
        "Term 应显示 TERMINAL 徽章"
    );
    assert_eq!(
        node_type_label(NodeType::Agent),
        "AGENT",
        "Agent 应显示 AGENT 徽章"
    );
    assert_eq!(
        node_type_label(NodeType::Preview),
        "PREVIEW",
        "Preview 应显示 PREVIEW 徽章"
    );
    assert_eq!(
        node_type_label(NodeType::Browser),
        "BROWSER",
        "Browser 应显示 BROWSER 徽章"
    );
    assert_eq!(
        node_type_label(NodeType::Rect),
        "NODE",
        "Rect 应显示 NODE 徽章"
    );
    assert_ne!(
        node_type_label(NodeType::Term),
        "Terminal",
        "类型徽章不得复用混合大小写业务标题"
    );
}

/// 用户实拍中的卡片标题栏应使用分层毛玻璃材质，而不是在外壳上再叠一层实色白面。
///
/// 数据构造（含关键数值推导）：
///   外壳 alpha = 0x54 / 0xff ≈ 33%。
///   标题层 top alpha = 0x70 / 0xff ≈ 44%。
///   两层组合 alpha = 1 - (1 - 0x54/255) × (1 - 0x70/255) ≈ 62%。
///   标题层 bottom alpha = 0x38 / 0xff ≈ 22%，用于形成冷白雾化梯度。
///
/// 执行过程：
///   1. 读取 cards/chrome.rs 的共享标题材质实现。
///   2. 读取 canvas/view.rs 的标题栏挂载点。
///   3. 检查标题栏使用共享渐变背景和 0x70 白色底部分界。
///
/// 预期结果：
///   - 正断言：共享材质使用 linear_gradient，底部颜色为 0x38 冷雾层。
///   - 正断言：Canvas 标题栏使用共享材质并渲染半透明底部分界。
///   - 负断言：标题栏不得继续直接 `.bg(rgba(visual.title_bar_color))` 叠成近实色白面。
#[test]
fn test_product_ui_v2_card_titlebar_uses_layered_frosted_material() {
    let chrome = include_str!("../../src/cards/chrome.rs");
    let canvas = include_str!("../../src/canvas/view.rs");

    assert!(
        chrome.contains("pub fn card_title_background")
            && chrome.contains("linear_gradient(")
            && chrome.contains("with_alpha(theme.canvas_to, 0x38)"),
        "共享卡片标题材质应使用 top→0x38 冷雾渐变模拟毛玻璃层"
    );
    assert!(
        canvas.contains(".bg(card_title_background(visual.title_bar_color, theme))")
            && canvas.contains(".border_b_1()")
            && canvas.contains("with_alpha(theme.edge_highlight, 0x70)"),
        "Canvas 标题栏应挂载共享毛玻璃渐变与 0x70 半透明底部分界"
    );
    assert!(
        !canvas.contains(".bg(rgba(visual.title_bar_color))"),
        "标题栏不得继续直接叠加实色 title_bar_color，否则壁纸无法透出"
    );
}
