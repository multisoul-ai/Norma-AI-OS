use super::layout::{ACTION_CARD_HEIGHT, ACTION_CARD_WIDTH, CONTENT_WIDTH};
use super::*;
use std::path::PathBuf;
use workspace::{RecentWorkspaces, WorkspaceSource};

/// L-1: 1440x900 宽屏 Launchpad 不应再显示右侧静态卡片区。
///
/// 数据构造（含关键数值的推导过程）：
///   window = 1440x900
///   spec 锚点：content left ≈118、top ≈186、width ≈560
///   旧 preview 卡片数量 = 4（Terminal / Codex Agent / Preview / Workspace shape）
///   目标 preview 卡片数量 = 0（用户要求去除右侧显示的卡片）
///
/// 执行过程：
///   1. 调用 launchpad_layout_for_window(1440, 900)
///   2. 检查布局模式、左侧内容锚点仍稳定、右侧卡片模块未接入
///
/// 预期结果：
///   - 正断言：mode 为 Wide，主内容接近既有视觉锚点
///   - 正断言：渲染入口不再声明 preview 模块或挂载 preview_cluster
///   - 负断言：右侧卡片模块不应继续接入 Launchpad
#[test]
fn test_launchpad_layout_wide_hides_right_preview_cards() {
    let layout = launchpad_layout_for_window(1440.0, 900.0);
    let source_mod = include_str!("../../src/workspace/launchpad/mod.rs");

    assert_eq!(
        layout.mode,
        LaunchpadLayoutMode::Wide,
        "1440px 宽窗口应保留 Wide 布局模式以稳定主内容锚点"
    );
    assert_close(
        layout.content_left,
        118.0,
        2.0,
        "content_left 应接近方案 1 原型 118px 锚点",
    );
    assert_close(
        layout.content_top,
        186.0,
        4.0,
        "content_top 应接近方案 1 原型 186px 锚点",
    );
    assert_close(
        layout.content_width,
        560.0,
        0.5,
        "content_width 应保持 560px 主列宽，去除右侧卡片不应挤压主入口",
    );
    assert!(
        !source_mod.contains("mod preview"),
        "Launchpad 不应继续声明右侧静态卡片 preview 模块"
    );
    assert!(
        !source_mod.contains("preview_cluster"),
        "Launchpad render_launchpad 不应继续挂载右侧静态卡片 preview_cluster"
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
fn test_launchpad_layout_compact_hides_preview_and_keeps_content_safe() {
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

/// L-4: Recent Projects 数据源应只取前 6 条并保持 RecentWorkspaces 的排序结果。
///
/// 数据构造（含关键数值的推导过程）：
///   构造 7 条记录，timestamp 从 100 到 106 递增
///   RecentWorkspaces 会按 last_opened_at_unix 降序排序
///   top(6) 应保留 project-6、project-5、project-4、project-3、project-2、project-1
///
/// 执行过程：
///   1. 依次 record_opened_at 7 个 workspace
///   2. 调用 recent_project_records_for_launchpad
///   3. 检查返回数量、顺序和被截断项
///
/// 预期结果：
///   - 正断言：返回 6 条，第一条是最新 project-6
///   - 正断言：第六条是 project-1
///   - 负断言：project-0 不应出现在 Launchpad 列表中（top(6) 截断）
#[test]
fn test_recent_project_records_for_launchpad_keeps_top_six() {
    let mut recent = RecentWorkspaces::default();
    for index in 0..7 {
        recent.record_opened_at(
            PathBuf::from(format!("/tmp/project-{index}")),
            WorkspaceSource::Open,
            100 + index,
        );
    }

    let records = recent_project_records_for_launchpad(&recent);

    assert_eq!(
        records.len(),
        6,
        "Launchpad recent list 应只渲染 top(6) 记录"
    );
    assert_eq!(
        records[0].display_name, "project-6",
        "最新的 project-6 应排在 Launchpad recent 第一条"
    );
    assert_eq!(
        records[5].display_name, "project-1",
        "第 6 条应是 top(6) 中最后保留的 project-1"
    );
    assert!(
        records
            .iter()
            .all(|record| record.display_name != "project-0"),
        "project-0 超出 top(6)，不应出现在 Launchpad recent 列表"
    );
}

/// L-5: Launchpad action cards 应为图标、标题和副标题预留完整垂直空间。
///
/// 数据构造（含关键数值的推导过程）：
///   icon line-height     = 20px（单行 glyph，不能吃掉标题空间）
///   title line-height    = 20px（Open Project / Clone Repo 主标题）
///   meta line-height     = 16px（Choose a local workspace / Remote setup soon）
///   vertical gaps        = 6px + 6px（图标到标题、标题到副标题）
///   vertical padding     = 12px * 2（上下可读安全边距）
///   required height      = 20 + 20 + 16 + 6 + 6 + 24 = 92px
///   action row width     = 180px * 2 + 14px = 374px，应小于 CONTENT_WIDTH 560px
///
/// 执行过程：
///   1. 读取 ACTION_CARD_HEIGHT / ACTION_CARD_WIDTH / CONTENT_WIDTH 常量
///   2. 用固定排版预算计算 action card 最小高度
///   3. 检查双卡片横排仍在主内容列宽内
///
/// 预期结果：
///   - 正断言：ACTION_CARD_HEIGHT 至少为 92px，副标题不会被底部裁剪
///   - 正断言：两个 action card 加 14px 间距后仍可放入主内容列
///   - 负断言：ACTION_CARD_HEIGHT 不应停留在 82px 这类会裁剪三行内容的高度
#[test]
fn test_launchpad_action_cards_reserve_text_budget() {
    let icon_line_height = 20.0;
    let title_line_height = 20.0;
    let meta_line_height = 16.0;
    let icon_to_title_gap = 6.0;
    let title_to_meta_gap = 6.0;
    let vertical_padding = 12.0 * 2.0;
    let required_height = icon_line_height
        + title_line_height
        + meta_line_height
        + icon_to_title_gap
        + title_to_meta_gap
        + vertical_padding;
    let action_row_width = ACTION_CARD_WIDTH * 2.0 + 14.0;

    assert!(
        ACTION_CARD_HEIGHT >= required_height,
        "Action card height 应至少覆盖 20 + 20 + 16 + 6 + 6 + 24 = 92px 的三行排版预算"
    );
    assert!(
        action_row_width <= CONTENT_WIDTH,
        "两个 action card 加 14px 间距后应小于 560px 主列宽，避免横向挤压"
    );
    assert_ne!(
        ACTION_CARD_HEIGHT, 82.0,
        "Action card 不应继续使用 82px 高度，否则副标题容易被底部裁剪"
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

/// L-9: Launchpad 背景应复用画布的视频感知壁纸层，而非静态 `img()`。
///
/// 背景：画布 chrome 通过 `render_wallpaper_layer()` 按 `WallpaperKind` 分支
/// （Video→`surface()`，Static/Gif→`img()` + 源派生 id），并在每帧调用
/// `sync_video_wallpaper(cx)` 驱动视频解码器生命周期。Launchpad 此前只渲染
/// 单个 `img(image_source_for_render())`，而该工厂对 Video 故意回退静态 PNG，
/// 导致 launchpad 不支持 mp4/mov，且 gif 切换存在 `frame_index` 越界 panic
/// 隐患。此测试守护「launchpad 与画布走同一条壁纸渲染路径」这一不变量。
///
/// 数据构造（含关键数值的推导过程）：
///   source_mod      = launchpad/mod.rs（render_launchpad 组装入口）
///   source_backdrop = launchpad/backdrop.rs（背景层构建）
///   复用契约        = render_launchpad 调 render_wallpaper_layer + sync_video_wallpaper
///   解耦契约        = backdrop 不再内联 img() / 不再接收 ImageSource
///
/// 执行过程：
///   1. 读取 launchpad/mod.rs，确认 render_launchpad 调用 render_wallpaper_layer
///   2. 读取 launchpad/mod.rs，确认 render_launchpad 调用 sync_video_wallpaper
///   3. 读取 launchpad/backdrop.rs，确认不再内联 img() 渲染静态图片
///   4. 读取 launchpad/backdrop.rs，确认签名不再接收 ImageSource
///
/// 预期结果：
///   - 正断言：mod.rs 包含 render_wallpaper_layer（走共享视频感知渲染路径）
///   - 正断言：mod.rs 包含 sync_video_wallpaper（驱动视频解码生命周期）
///   - 负断言：backdrop.rs 不应包含 img(（不再内联静态图片元素，否则 mp4 不支持）
///   - 负断言：backdrop.rs 不应包含 ImageSource（签名改为接收上层构建的壁纸层 Div）
#[test]
fn test_launchpad_backdrop_reuses_video_aware_wallpaper_layer() {
    let source_mod = include_str!("../../src/workspace/launchpad/mod.rs");
    let source_backdrop = include_str!("../../src/workspace/launchpad/backdrop.rs");

    assert!(
        source_mod.contains("render_wallpaper_layer"),
        "Launchpad render_launchpad 应复用画布的 render_wallpaper_layer，\
         否则 Video 类型无法走 surface() 渲染路径"
    );
    assert!(
        source_mod.contains("sync_video_wallpaper"),
        "Launchpad render_launchpad 应每帧调用 sync_video_wallpaper 驱动视频解码器，\
         否则 mp4/mov 永远不会有解码帧可渲染"
    );
    assert!(
        !source_backdrop.contains("img("),
        "Launchpad backdrop 不应再内联 img() 渲染静态图片——这会让 Video 回退到 PNG，\
         且 gif 切换时缺少源派生 id 触发 frame_index 越界 panic"
    );
    assert!(
        !source_backdrop.contains("ImageSource"),
        "Launchpad backdrop 签名不应再接收 ImageSource——壁纸层应由上层 \
         render_wallpaper_layer 构建，backdrop 只负责挂载与遮罩"
    );
}

/// L-10: Workspace/Launchpad 页面不应再挂载右侧静态卡片预览模块。
///
/// 数据构造（含关键数值的推导过程）：
///   source_mod = launchpad/mod.rs（真实 render_launchpad 组装入口）
///   旧右侧卡片模块 = preview.rs
///   旧挂载函数数量 = 1（preview_cluster）
///   目标挂载函数数量 = 0（右侧静态卡片区完全移除）
///
/// 执行过程：
///   1. 读取 launchpad/mod.rs，确认不再声明 mod preview
///   2. 读取 launchpad/mod.rs，确认不再 import / child(preview_cluster)
///   3. 确认仍保留 launchpad_brand，避免测试因页面主体被整体删除而误通过
///
/// 预期结果：
///   - 正断言：Launchpad 仍保留品牌和主入口区域
///   - 负断言：Launchpad 不应继续声明 preview 模块
///   - 负断言：Launchpad 不应继续调用 preview_cluster 挂载右侧卡片
#[test]
fn test_launchpad_drops_right_preview_card_cluster() {
    let source_mod = include_str!("../../src/workspace/launchpad/mod.rs");

    assert!(
        source_mod.contains("launchpad_brand"),
        "Launchpad 应继续渲染品牌和主入口区域，去除右侧卡片不应删除页面主体"
    );
    assert!(
        !source_mod.contains("mod preview"),
        "Launchpad 不应继续声明右侧静态卡片 preview 模块"
    );
    assert!(
        !source_mod.contains("preview_cluster"),
        "Launchpad render_launchpad 不应继续挂载右侧静态卡片 preview_cluster"
    );
}

fn assert_close(actual: f32, expected: f32, tolerance: f32, message: &str) {
    let delta = (actual - expected).abs();
    assert!(
        delta <= tolerance,
        "{message}: actual={actual}, expected={expected}, tolerance={tolerance}, delta={delta}"
    );
}
