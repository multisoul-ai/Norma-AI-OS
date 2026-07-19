use super::layout::{ACTION_CARD_HEIGHT, ACTION_CARD_WIDTH, CONTENT_WIDTH};
use super::*;
use std::path::PathBuf;
use workspace::{RecentWorkspaces, WorkspaceSource};

/// L-1: 1440x900 宽屏与 960x700 紧凑 Launchpad 都应稳定放置主内容列。
///
/// 数据构造（含关键数值的推导过程）：
///   window = 1440x900
///   spec 锚点：content left ≈96、top ≈92、width ≈500
///   Compact 960px：内容列居中，宽度仍为 min(500, 960 - 24×2) = 500px
///
/// 执行过程：
///   1. 调用 launchpad_layout_for_window(1440, 900)
///   2. 检查 Wide 布局模式与主内容锚点
///   3. 对 960x700 再计算一次，验证 Compact 布局仍保留主内容宽度
///
/// 预期结果：
///   - 正断言：Wide 模式使用 96/92/500px 主内容锚点
///   - 正断言：Compact 模式仍保留 500px 主内容宽度
///   - 负断言：Compact 模式不得错误沿用 Wide 模式
#[test]
fn test_launchpad_layout_wide_and_compact_keep_main_content_stable() {
    let layout = launchpad_layout_for_window(1440.0, 900.0);
    let compact = launchpad_layout_for_window(960.0, 700.0);

    assert_eq!(
        layout.mode,
        LaunchpadLayoutMode::Wide,
        "1440px 宽窗口应保留 Wide 布局模式以稳定主内容锚点"
    );
    assert_close(
        layout.content_left,
        96.0,
        3.0,
        "content_left 应接近 Native Launchpad 原型 96px 锚点",
    );
    assert_close(
        layout.content_top,
        92.0,
        4.0,
        "content_top 应接近 Native Launchpad 原型 92px 锚点",
    );
    assert_close(
        layout.content_width,
        500.0,
        4.0,
        "content_width 应接近目标主内容列 500px",
    );
    assert_close(
        compact.content_width,
        500.0,
        0.5,
        "960px Compact Launchpad 仍应保留 500px 主内容宽度",
    );
    assert_ne!(
        compact.mode,
        LaunchpadLayoutMode::Wide,
        "960px Compact Launchpad 不得错误沿用 Wide 模式"
    );
}

/// L-2: 960x700 窄窗口 Launchpad 应保持主入口位于安全边距内。
///
/// 数据构造（含关键数值的推导过程）：
///   window = 960x700
///   安全边距 = 24px * 2，因此主内容最大可用宽度 = 960 - 48 = 912
///   spec 要求 width < 1180 时进入 Compact
///
/// 执行过程：
///   1. 调用 launchpad_layout_for_window(960, 700)
///   2. 检查 Compact 模式、content 位于安全边距内
///
/// 预期结果：
///   - 正断言：mode 为 Compact
///   - 正断言：content 左右边界都在 24px 安全边距内
///   - 负断言：content_width 不应超过 912px
#[test]
fn test_launchpad_layout_compact_keeps_content_safe() {
    let layout = launchpad_layout_for_window(960.0, 700.0);
    let safe_left = 24.0;
    let safe_right = 960.0 - 24.0;

    assert_eq!(
        layout.mode,
        LaunchpadLayoutMode::Compact,
        "960px 宽窗口应使用 Compact 布局"
    );
    assert!(
        layout.content_left >= safe_left,
        "Compact content_left 应不小于 24px 安全边距"
    );
    assert!(
        layout.content_left + layout.content_width <= safe_right,
        "Compact content 右边界应不超过窗口右侧 24px 安全边距"
    );
    assert!(
        layout.content_width <= 912.0,
        "Compact content_width 不应超过 960 - 24*2 = 912px"
    );
}

/// L-3: 1180x640 矮窗口 Launchpad 应上移主内容，避免 action/recent 被底部裁剪。
///
/// 数据构造（含关键数值的推导过程）：
///   window = 1180x640
///   640px 高度比原型 900px 少 260px，需要降低 top
///   期望 top 不超过 120px，给 brand/action/recent 留出垂直空间
///
/// 执行过程：
///   1. 调用 launchpad_layout_for_window(1180, 640)
///   2. 检查 content_top 落在可读安全范围内
///
/// 预期结果：
///   - 正断言：content_top <= 120px
///   - 正断言：content_top >= 72px，避免顶到 toolbar 区域
///   - 负断言：content_top 不应保持原型 186px，否则矮窗口下会明显裁剪
#[test]
fn test_launchpad_layout_short_window_moves_content_up() {
    let layout = launchpad_layout_for_window(1180.0, 640.0);

    assert!(
        layout.content_top <= 120.0,
        "640px 矮窗口 content_top 应上移到 120px 以内"
    );
    assert!(
        layout.content_top >= 72.0,
        "640px 矮窗口 content_top 仍应低于顶部 toolbar 安全区"
    );
    assert_ne!(
        layout.content_top, 186.0,
        "矮窗口不应继续使用 900px 原型中的 186px content_top"
    );
}

/// L-4: Recent Projects 数据源应保留全部已持久化记录，让超过可视区的项目可滚动访问。
///
/// 数据构造（含关键数值的推导过程）：
///   构造 14 条记录，timestamp 从 100 到 113 递增
///   RecentWorkspaces 会按 last_opened_at_unix 降序排序
///   每行高度 = 64px，13 条分隔线不改变滚动内容数量
///   14 行自然高度 = 14 * 64 = 896px，超过 Launchpad Recent 区的可视高度，
///   因此必须将全部记录交给 overflow_scroll 容器，而不是在数据源阶段截为 6 条
///
/// 执行过程：
///   1. 依次 record_opened_at 14 个 workspace
///   2. 调用 recent_project_records_for_launchpad
///   3. 检查返回数量、首尾顺序和第 7 条之后的记录
///
/// 预期结果：
///   - 正断言：返回 14 条，第一条是最新 project-13
///   - 正断言：第 7 条 project-7 与最后一条 project-0 都存在，供滚动后访问
///   - 负断言：数据源不应截断为 6 条，否则 overflow_scroll 没有完整内容
#[test]
fn test_recent_project_records_for_launchpad_keeps_all_records_for_scrolling() {
    let mut recent = RecentWorkspaces::default();
    for index in 0..14 {
        recent.record_opened_at(
            PathBuf::from(format!("/tmp/project-{index}")),
            WorkspaceSource::Open,
            100 + index,
        );
    }

    let records = recent_project_records_for_launchpad(&recent);

    assert_eq!(
        records.len(),
        14,
        "Launchpad recent list 应保留全部 14 条记录，供滚动容器渲染"
    );
    assert_eq!(
        records[0].display_name, "project-13",
        "最新的 project-13 应排在 Launchpad recent 第一条"
    );
    assert_eq!(
        records[6].display_name, "project-7",
        "第 7 条 project-7 应保留，滚动后必须可访问"
    );
    assert_eq!(
        records[13].display_name, "project-0",
        "最早的 project-0 应保留在列表末尾，滚动到底部后必须可访问"
    );
    assert_ne!(
        records.len(),
        6,
        "Recent Projects 不应截断为 6 条，否则列表没有完整滚动内容"
    );
}

/// L-5: Launchpad action pills 应使用 44px 原生胶囊，并完整容纳图标与标题。
///
/// 数据构造（含关键数值的推导过程）：
///   icon box             = 22px（单行 glyph）
///   vertical padding     = 11px * 2
///   required height      = 22 + 11*2 = 44px
///   action row width     = 168px * 2 + 14px = 350px，应小于 CONTENT_WIDTH 500px
///
/// 执行过程：
///   1. 读取 ACTION_CARD_HEIGHT / ACTION_CARD_WIDTH / CONTENT_WIDTH 常量
///   2. 用固定排版预算计算 action pill 精确高度
///   3. 检查双卡片横排仍在主内容列宽内
///
/// 预期结果：
///   - 正断言：ACTION_CARD_HEIGHT 精确为 44px，命中区与原型一致
///   - 正断言：两个 action card 加 14px 间距后仍可放入主内容列
///   - 负断言：ACTION_CARD_HEIGHT 不应继续使用旧 92px 大卡片
#[test]
fn test_launchpad_action_cards_reserve_text_budget() {
    let icon_box = 22.0;
    let vertical_padding = 11.0 * 2.0;
    let required_height = icon_box + vertical_padding;
    let action_row_width = ACTION_CARD_WIDTH * 2.0 + 14.0;

    assert_eq!(
        ACTION_CARD_HEIGHT, required_height,
        "Action pill height 应等于 22 + 11*2 = 44px 的单行排版预算"
    );
    assert!(
        action_row_width <= CONTENT_WIDTH,
        "两个 action pill 加 14px 间距后应小于 500px 主列宽，避免横向挤压"
    );
    assert_ne!(
        ACTION_CARD_HEIGHT, 92.0,
        "Action pill 不应继续使用旧 92px 大卡片高度"
    );
}

/// L-7: 矮窗口 Launchpad 主内容列应限制最大高度，给 Recent Projects 留出可滚动空间。
///
/// 数据构造（含关键数值的推导过程）：
///   window = 1180x640
///   content_top ≈ 84..120（layout 会把主列上移）
///   SAFE_EDGE = 24px
///   content_max_height = 640 - content_top - 24
///
/// 执行过程：
///   1. 调用 launchpad_layout_for_window(1180, 640)
///   2. 检查 content_max_height 是否小于主列自然高度预算
///
/// 预期结果：
///   - 正断言：content_max_height >= 320px，Recent 区仍保留最小可用高度
///   - 正断言：content_max_height <= 640 - 24，避免主列超出窗口底部
///   - 负断言：content_max_height 不应为 0，否则 Recent 无法滚动
#[test]
fn test_launchpad_layout_short_window_caps_content_height() {
    let layout = launchpad_layout_for_window(1180.0, 640.0);
    let expected_max = 640.0 - layout.content_top - 24.0;

    assert!(
        layout.content_max_height >= 320.0,
        "640px 矮窗口下 Recent Projects 至少应保留 320px 可滚动高度"
    );
    assert_close(
        layout.content_max_height,
        expected_max,
        0.5,
        "content_max_height 应等于 window_height - content_top - SAFE_EDGE",
    );
    assert!(
        layout.content_max_height > 0.0,
        "content_max_height 不应为 0，否则 Recent Projects 无法建立滚动上下文"
    );
}

/// L-8: Recent Projects 列表应使用 GPUI overflow_scroll，在内容超出时接管滚轮。
///
/// 数据构造（含关键数值的推导过程）：
///   source_recent = launchpad/recent.rs
///   source_mod    = launchpad/mod.rs
///   scroll id     = launchpad-recent-list-scroll
///
/// 执行过程：
///   1. 读取 recent.rs，确认列表容器声明 overflow_scroll 与稳定 id
///   2. 读取 mod.rs，确认主内容列使用 layout.content_max_height 约束高度
///
/// 预期结果：
///   - 正断言：recent 列表容器包含 overflow_scroll 与 launchpad-recent-list-scroll
///   - 正断言：主内容列绑定 content_max_height
///   - 负断言：recent 列表不应只是裸 flex_col，否则滚轮无法在该上下文内滚动
#[test]
fn test_recent_projects_list_uses_overflow_scroll_container() {
    let source_recent = include_str!("../../src/workspace/launchpad/recent.rs");
    let source_mod = include_str!("../../src/workspace/launchpad/mod.rs");

    assert!(
        source_recent.contains("launchpad-recent-list-scroll"),
        "Recent Projects 列表应有稳定 scroll 容器 id"
    );
    assert!(
        source_recent.contains("overflow_scroll"),
        "Recent Projects 列表容器应启用 overflow_scroll 以支持上下文滚轮"
    );
    assert!(
        source_mod.contains("content_max_height"),
        "Launchpad 主内容列应使用 content_max_height 限制高度，触发 Recent 区滚动"
    );
}

/// L-6: Workspace/Launchpad 页面不应再显示顶部静态菜单预览。
///
/// 数据构造（含关键数值的推导过程）：
///   source_mod      = launchpad/mod.rs（真实 render_launchpad 组装入口）
///   source_backdrop = launchpad/backdrop.rs（旧 toolbar_preview / chip 定义所在文件）
///   旧菜单项目数量 = 6（CANVAS / + / Claude Code / Codex / Preview / Settings）
///   目标菜单项目数量 = 0（用户要求去除截图里的顶部菜单）
///
/// 执行过程：
///   1. 读取 launchpad/mod.rs，确认 render_launchpad 不再 child(toolbar_preview(...))
///   2. 读取 launchpad/backdrop.rs，确认旧 toolbar_preview 和 toolbar_chip 已移除
///   3. 检查旧菜单文案不再保留在 backdrop 渲染源码中
///
/// 预期结果：
///   - 正断言：Launchpad 仍保留 launchpad_backdrop 背景入口
///   - 负断言：Launchpad 不应继续挂载 toolbar_preview
///   - 负断言：旧顶部菜单 6 个 chip 文案不应留在 backdrop 渲染源码
#[test]
fn test_launchpad_drops_static_top_toolbar_preview() {
    let source_mod = include_str!("../../src/workspace/launchpad/mod.rs");
    let source_backdrop = include_str!("../../src/workspace/launchpad/backdrop.rs");
    let legacy_menu_labels = [
        "\"CANVAS\"",
        "\"+\"",
        "\"Claude Code\"",
        "\"Codex\"",
        "\"Preview\"",
        "\"Settings\"",
    ];

    assert!(
        source_mod.contains("launchpad_backdrop("),
        "Launchpad 应继续渲染背景层，去除顶部菜单不应移除页面背景"
    );
    assert!(
        !source_mod.contains("toolbar_preview("),
        "Launchpad render_launchpad 不应继续挂载截图中的顶部静态菜单"
    );
    assert!(
        !source_backdrop.contains("fn toolbar_preview"),
        "旧 toolbar_preview 函数应从 backdrop 中移除，避免菜单被重新挂载"
    );
    assert!(
        !source_backdrop.contains("fn toolbar_chip"),
        "旧 toolbar_chip 函数应从 backdrop 中移除，避免保留无用菜单构件"
    );

    for label in legacy_menu_labels {
        assert!(
            !source_backdrop.contains(label),
            "Launchpad backdrop 不应继续保留旧顶部菜单文案: {label}"
        );
    }
}

/// L-10: Product UI V2 Launchpad 应完全移除右侧 Workspace preview 区域。
///
/// 数据构造（含关键数值的推导过程）：
///   source_mod = launchpad/mod.rs（真实 render_launchpad 组装入口）
///   右侧 preview 模块数量 = 0
///   preview 挂载函数数量 = 0
///   旧 preview_cluster 数量 = 0
///
/// 执行过程：
///   1. 读取 launchpad/mod.rs，确认品牌与主入口仍存在。
///   2. 搜索 preview 模块声明和 workspace_preview 挂载。
///   3. 搜索旧 preview_cluster，防止任何右侧示意区回流。
///
/// 预期结果：
///   - 正断言：Launchpad 继续渲染品牌和主入口区域。
///   - 负断言：不得声明或挂载 workspace_preview。
///   - 负断言：不得恢复旧 preview_cluster。
#[test]
fn test_launchpad_removes_entire_workspace_preview_region() {
    let source_mod = include_str!("../../src/workspace/launchpad/mod.rs");

    assert!(
        source_mod.contains("launchpad_brand"),
        "Launchpad 应继续渲染品牌和主入口区域"
    );
    assert!(
        !source_mod.contains("mod preview"),
        "Launchpad 不应继续声明用户要求删除的 preview 模块"
    );
    assert!(
        !source_mod.contains("workspace_preview("),
        "Launchpad render_launchpad 不应继续挂载 Workspace preview 区域"
    );
    assert!(
        !source_mod.contains("preview_cluster"),
        "Launchpad render_launchpad 不应继续挂载右侧静态卡片 preview_cluster"
    );
}

/// L-11: Launchpad 品牌图应使用独立产品 Logo，而不是复用 App icon。
///
/// 数据构造（含关键数值的推导过程）：
///   source_actions = launchpad/actions.rs（真实品牌区元素树）
///   logo const     = APP_LOGO_ASSET（目标产品 logo 路径 icons/logo.png）
///   icon const     = APP_ICON_ASSET（目标 app icon 路径 icons/app-icon.png）
///   品牌图片区数量 = 1 个 img(...) 调用，位于 44x44 品牌容器中
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 launchpad/actions.rs
///   2. 确认文件 import / 使用 APP_LOGO_ASSET
///   3. 确认品牌图片区调用 img(APP_LOGO_ASSET)
///   4. 确认品牌图片区不再调用 img(APP_ICON_ASSET)
///
/// 预期结果：
///   - 正断言：launchpad brand 使用 APP_LOGO_ASSET
///   - 正断言：品牌图片由 img(APP_LOGO_ASSET) 渲染
///   - 负断言：品牌图片不应继续由 img(APP_ICON_ASSET) 渲染
#[test]
fn test_launchpad_brand_uses_logo_asset_not_app_icon() {
    let source_actions = include_str!("../../src/workspace/launchpad/actions.rs");

    assert!(
        source_actions.contains("APP_LOGO_ASSET"),
        "Launchpad brand 应导入并使用 APP_LOGO_ASSET 作为产品 logo"
    );
    assert!(
        source_actions.contains("img(APP_LOGO_ASSET)"),
        "Launchpad 品牌图片区应通过 img(APP_LOGO_ASSET) 渲染产品 logo"
    );
    assert!(
        !source_actions.contains("img(APP_ICON_ASSET)"),
        "Launchpad 品牌图片区不应继续复用 APP_ICON_ASSET；app icon 与产品 logo 需分离"
    );
}

fn assert_close(actual: f32, expected: f32, tolerance: f32, message: &str) {
    let delta = (actual - expected).abs();
    assert!(
        delta <= tolerance,
        "{message}: actual={actual}, expected={expected}, tolerance={tolerance}, delta={delta}"
    );
}
