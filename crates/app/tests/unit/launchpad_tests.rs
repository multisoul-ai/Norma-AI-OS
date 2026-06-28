use super::*;
use std::path::PathBuf;
use workspace::{RecentWorkspaces, WorkspaceSource};

/// L-1: 1440x900 宽屏 Launchpad 应采用方案 1 双区布局并接近原型视觉锚点。
///
/// 数据构造（含关键数值的推导过程）：
///   window = 1440x900
///   spec 锚点：content left ≈118、top ≈186、width ≈560
///   spec 锚点：preview right ≈112、width ≈520，因此 preview_left ≈ 1440 - 112 - 520 = 808
///
/// 执行过程：
///   1. 调用 launchpad_layout_for_window(1440, 900)
///   2. 检查布局模式、preview 可见性、左侧内容和右侧预览位置
///
/// 预期结果：
///   - 正断言：mode 为 Wide，preview_visible = true
///   - 正断言：content / preview 接近方案 1 视觉锚点
///   - 负断言：preview 不应与 content 水平重叠
#[test]
fn test_launchpad_layout_wide_matches_option_one_visual_anchors() {
    let layout = launchpad_layout_for_window(1440.0, 900.0);

    assert_eq!(
        layout.mode,
        LaunchpadLayoutMode::Wide,
        "1440px 宽窗口应使用 Wide 双区布局"
    );
    assert!(
        layout.preview_visible,
        "1440px 宽窗口应显示右侧静态 preview 区"
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
        "content_width 应保持方案 1 的 560px 主列宽",
    );
    assert_close(
        layout.preview_left,
        808.0,
        6.0,
        "preview_left 应接近 1440 - 112 - 520 = 808px",
    );
    assert!(
        layout.preview_left >= layout.content_left + layout.content_width + 96.0,
        "右侧 preview 不应与左侧 content 重叠，应至少保留 96px 空隙"
    );
}

/// L-2: 960x700 窄窗口 Launchpad 应隐藏右侧 preview 并保持主入口可用。
///
/// 数据构造（含关键数值的推导过程）：
///   window = 960x700
///   安全边距 = 24px * 2，因此主内容最大可用宽度 = 960 - 48 = 912
///   spec 要求 width < 1180 时进入 Compact 并隐藏 preview
///
/// 执行过程：
///   1. 调用 launchpad_layout_for_window(960, 700)
///   2. 检查 Compact 模式、preview 隐藏、content 位于安全边距内
///
/// 预期结果：
///   - 正断言：mode 为 Compact，preview_visible = false
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
        !layout.preview_visible,
        "Compact 布局不应显示右侧静态 preview，避免核心入口被压缩"
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

/// L-4: Recent Projects 数据源应只取前 5 条并保持 RecentWorkspaces 的排序结果。
///
/// 数据构造（含关键数值的推导过程）：
///   构造 7 条记录，timestamp 从 100 到 106 递增
///   RecentWorkspaces 会按 last_opened_at_unix 降序排序
///   top(5) 应保留 project-6、project-5、project-4、project-3、project-2
///
/// 执行过程：
///   1. 依次 record_opened_at 7 个 workspace
///   2. 调用 recent_project_records_for_launchpad
///   3. 检查返回数量、顺序和被截断项
///
/// 预期结果：
///   - 正断言：返回 5 条，第一条是最新 project-6
///   - 正断言：第五条是 project-2
///   - 负断言：project-1 / project-0 不应出现在 Launchpad 列表中
#[test]
fn test_recent_project_records_for_launchpad_keeps_top_five() {
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
        5,
        "Launchpad recent list 应只渲染 top(5) 记录"
    );
    assert_eq!(
        records[0].display_name, "project-6",
        "最新的 project-6 应排在 Launchpad recent 第一条"
    );
    assert_eq!(
        records[4].display_name, "project-2",
        "第 5 条应是 top(5) 中最后保留的 project-2"
    );
    assert!(
        records
            .iter()
            .all(|record| record.display_name != "project-1"),
        "project-1 超出 top(5)，不应出现在 Launchpad recent 列表"
    );
    assert!(
        records
            .iter()
            .all(|record| record.display_name != "project-0"),
        "project-0 超出 top(5)，不应出现在 Launchpad recent 列表"
    );
}

fn assert_close(actual: f32, expected: f32, tolerance: f32, message: &str) {
    let delta = (actual - expected).abs();
    assert!(
        delta <= tolerance,
        "{message}: actual={actual}, expected={expected}, tolerance={tolerance}, delta={delta}"
    );
}
