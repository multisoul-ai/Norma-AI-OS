use super::*;

/// Product UI V2 卡片外壳：1× 缩放时应使用 22px 圆角与 32px 标题栏。
///
/// 数据构造（含关键数值推导）：
///   zoom = 1.0
///   目标 card radius = 22px（Canvas V3 `.canvas-object`）
///   目标 title bar   = 32px（紧凑 Canvas 标题栏）
///   旧值分别为 16px / 26px，无法形成参考图中的光学玻璃比例
///
/// 执行过程：
///   1. 调用 node_card_radius_px(1.0)。
///   2. 读取共享 TITLE_BAR_H。
///   3. 与旧 16/26px 做负断言，防止视觉回退。
///
/// 预期结果：
///   - 正断言：card radius=22px、TITLE_BAR_H=32px。
///   - 负断言：两者都不再是旧尺寸。
#[test]
fn test_product_ui_v2_card_shell_uses_22px_radius_and_32px_titlebar() {
    let radius = node_card_radius_px(1.0);

    assert_eq!(
        radius, 22.0,
        "1× Canvas card radius 应匹配 Product UI V2 的 22px"
    );
    assert_eq!(TITLE_BAR_H, 32.0, "Canvas card 标题栏应匹配紧凑布局的 32px");
    assert_ne!(radius, 16.0, "卡片圆角不得回退到旧 16px 外壳");
    assert_ne!(TITLE_BAR_H, 26.0, "标题栏不得回退到旧 26px 高度");
}

/// 卡片的半透明顶部圆角必须由同一标题材质绘制，不能叠加近不透明深色衬块。
///
/// 数据构造（含关键数值推导）：
///   zoom = 1.0 → card radius = 22px。
///   顶部毛玻璃采样区左右各避开 22px，以防 GPUI 0.2.2 的矩形裁剪把图片带到圆角外。
///   标题材质高度 = 32 × 1.0 = 32px，自身持有 top-left/top-right 圆弧。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 Canvas node shell 的 GPUI 构建代码。
///   2. 定位标题栏的毛玻璃渐变与顶部圆角声明。
///   3. 检查标题层没有额外挂载左右实色角块，同时 node shell 维持无背景。
///
/// 预期结果：
///   - 断言 A：node shell 存在 rounded 声明，圆角边界仍由外壳定义。
///   - 断言 B：单一标题材质自带顶部圆角并进入可视验收树。
///   - 断言 C：不存在旧近不透明角块；node shell 也不绘制全尺寸背景。
#[test]
fn test_product_ui_card_titlebar_has_no_opaque_corner_caps() {
    let canvas = include_str!("../../src/canvas/view.rs");
    let chrome = include_str!("../../src/cards/chrome.rs");
    let shell_start = canvas
        .find("let mut node_div = div()")
        .expect("Canvas 必须构造共享 node shell");
    let shell_end = canvas[shell_start..]
        .find(".on_hover(")
        .map(|offset| shell_start + offset)
        .expect("node shell 必须在 hover handler 前完成视觉样式声明");
    let shell = &canvas[shell_start..shell_end];

    assert!(
        shell.contains(".rounded(px(node_radius))"),
        "共享 node shell 必须拥有 node_radius 圆角，才能定义四个卡片边界"
    );
    assert!(
        canvas.contains(".debug_selector(|| \"card-titlebar\".to_string())")
            && canvas.contains(".rounded_t(px(node_title_radius_px(vp.zoom)))")
            && canvas.contains(".bg(card_title_background(visual.title_bar_color, theme))"),
        "单一标题材质必须自带顶部圆角与毛玻璃渐变，并可由 e2e 定位"
    );
    assert!(
        !canvas.contains("render_card_title_corner_backing")
            && !chrome.contains("pub fn render_card_title_corner_backing"),
        "Canvas 与共享 chrome 不得再挂载独立角块，否则会改变顶角的材质权重"
    );
    assert!(
        !chrome.contains("with_alpha(theme.node_fill, 0xf2)"),
        "卡片 chrome 不得再以 0xf2 近不透明 node_fill 绘制顶角深色块"
    );
    assert!(
        !shell.contains(".bg("),
        "共享 node shell 不得绘制全尺寸背景，否则 GPUI 会在圆角外留下矩形色块"
    );
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

/// 用户提供的 CSS 毛玻璃参考：原生卡片必须裁切并重复采样当前壁纸，而不能只叠半透明白色。
///
/// 数据构造（含关键数值推导）：
///   空间采样数 = 5（中心 + 4 个对角偏移）。
///   增量 alpha = 1、1/2、1/3、1/4、1/5；递推后每个样本最终权重均为 1/5。
///   最终覆盖率 = 5 × 1/5 = 100%，壁纸色彩不会被冷白底色替代。
///   采样偏移半径 = 5px，对应用户 CSS 示例的 `filter: blur(5px)` 视觉尺度。
///
/// 执行过程：
///   1. 读取 workspace chrome 的卡片背景采样实现。
///   2. 检查静态/GIF 壁纸使用同一 ImageSource 的 5 个空间偏移裁切样本。
///   3. 检查视频壁纸复用 CVPixelBuffer 的 `surface()`，不经过 CPU RGBA/RenderImage。
///   4. 检查 Canvas 节点先挂载毛玻璃背景，再绘制单一圆角标题材质和业务内容。
///
/// 预期结果：
///   - 正断言：存在 5 点偏移、等权全覆盖混合、卡片裁切层和视频 surface 采样。
///   - 正断言：Canvas 节点渲染顺序为 frosted backdrop → title layer → card body。
///   - 负断言：毛玻璃路径不得使用旧 0x24 固定透明度、CPU 帧拷贝或单层纯色背景。
#[test]
fn test_product_ui_card_glass_crops_and_spatially_samples_live_wallpaper() {
    let chrome = include_str!("../../src/app/chrome.rs");
    let canvas = include_str!("../../src/canvas/view.rs");
    let video = include_str!("../../src/app/video_wallpaper.rs");

    assert!(
        chrome.contains("FROST_SAMPLE_OFFSETS")
            && chrome.contains("render_card_frosted_backdrop")
            && chrome.contains("frost_sample_opacity(sample_index)"),
        "卡片毛玻璃应使用 5 点空间采样与逐层归一混合，而不是纯色遮罩"
    );
    assert!(
        !chrome.contains("WALLPAPER_FROST_SAMPLE_ALPHA"),
        "卡片毛玻璃不得继续使用 0x24 固定透明度，否则五层组合后仍会丢失约 47% 背景能量"
    );
    assert!(
        chrome.contains("image_source_for_render")
            && chrome.contains("render_video_wallpaper_frost_sample"),
        "静态/GIF 与视频壁纸都应复用当前背景源生成卡片裁切样本"
    );
    assert!(
        video.contains("surface(frame.buffer().clone())")
            && !video.contains("RenderImage")
            && !video.contains("to_rgba"),
        "视频毛玻璃必须继续复用 CVPixelBuffer surface，禁止回退为 CPU RGBA/RenderImage"
    );

    let backdrop = canvas
        .find("render_card_frosted_backdrop")
        .expect("Canvas 节点应挂载裁切壁纸毛玻璃层");
    let title_layer = canvas
        .find(".child(title_layer)")
        .expect("Canvas 节点应继续渲染单一圆角标题材质层");
    let body = canvas
        .find(".child(card_body)")
        .expect("Canvas 节点应继续渲染业务内容体");
    assert!(
        backdrop < title_layer && title_layer < body,
        "卡片绘制顺序必须是毛玻璃背景 → 标题层 → 业务内容，避免采样层遮挡交互"
    );
    assert!(
        !chrome.contains("read_to_end") && !chrome.contains("from_rgba"),
        "卡片毛玻璃不得读取或构造 CPU RGBA 帧，否则会破坏视频背景性能"
    );
}
