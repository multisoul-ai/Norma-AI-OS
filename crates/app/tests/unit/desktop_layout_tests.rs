use super::*;
use crate::canvas::layout_strategy::CanvasLayoutStrategy;
use canvas::{compute_configurable_grid_layout, Vec2};

/// 内嵌项目由外层预留窗口控件和 Dock，内部卡片从局部画布边缘开始排版。
///
/// 数据构造：窗口 1280×800；外层左右各 14、顶部 36、底部 91，局部尺寸 1252×673；
///   内部 gap=8，三列卡片宽=(1252-4×8)/3，高=673-2×8=657。
/// 执行过程：取得项目局部尺寸，选择内嵌安全边距，再由既有网格算法布局两张卡片。
/// 预期结果：第一张从局部 (8,8) 开始，底部 665 不超过 673；独立画布仍保留旧安全边距。
#[test]
fn desktop_embedded_layout_does_not_reserve_chrome_twice() {
    let scene = super::super::desktop_scene_rect(1280.0, 800.0);
    assert_eq!(
        scene.size,
        Vec2::new(1252.0, 673.0),
        "内嵌视区必须扣除外层空间"
    );
    let embedded = CanvasLayoutStrategy {
        chrome_in_parent: true,
        ..Default::default()
    };
    let config = embedded.grid_config_for_zoom(1.0);
    let rects = compute_configurable_grid_layout(
        &[1, 2],
        CanvasRect::new(0.0, 0.0, scene.size.x, scene.size.y),
        config,
    );
    assert_eq!(
        rects[0].bounds.origin,
        Vec2::new(8.0, 8.0),
        "内嵌布局不能叠加旧 96/86 边距"
    );
    assert_eq!(
        rects[1].bounds.max().y,
        665.0,
        "第二张卡片底部应在局部视区内"
    );
    assert!(
        rects[1].bounds.max().y < scene.size.y,
        "卡片不能被外层裁掉底部"
    );
    assert_eq!(
        CanvasLayoutStrategy::default()
            .grid_config_for_zoom(1.0)
            .safe_area
            .top,
        86.0,
        "独立画布仍需原有工具栏边距"
    );
    assert_ne!(
        config.safe_area,
        CanvasLayoutStrategy::default()
            .grid_config_for_zoom(1.0)
            .safe_area,
        "嵌入和独立视区不能重复使用相同边距"
    );
}

/// 项目区域复用响应式画布网格，按项目数和窗口宽度自动缩放、换行。
///
/// 数据构造：窗口 1280×800，内容 1252×673，gap=8；四项目最多三列、两行；
///   三列宽=(1252-4×8)/3，行高=(673-3×8)/2=324.5；窄窗口宽 600 只能容纳一列。
/// 执行过程：分别排列 1、2、4 个项目和窄窗的 2 项；检查行列变化和最后一个项目位置。
/// 预期结果：单项目填满内容宽，两项目并列，四项目换行；所有项目留在 Dock 上方，无旧固定全高行。
#[test]
fn desktop_project_regions_use_responsive_canvas_layout() {
    let one = project_area_layout(1280.0, 800.0, 1);
    assert_eq!(
        one[0],
        CanvasRect::new(22.0, 44.0, 1236.0, 657.0),
        "一个项目应填满可用区域"
    );
    let two = project_area_layout(1280.0, 800.0, 2);
    assert_eq!(
        two[1],
        CanvasRect::new(644.0, 44.0, 614.0, 657.0),
        "两个项目应均分宽度"
    );
    let four = project_area_layout(1280.0, 800.0, 4);
    assert_eq!(
        four[3].origin,
        Vec2::new(22.0, 376.5),
        "第四个项目应进入第二行"
    );
    assert_eq!(four[3].max().y, 701.0, "最后项目必须在底部安全区之前结束");
    assert_ne!(
        four[0].size.y, two[0].size.y,
        "项目增加后不能保留旧的全高卡片"
    );
    let narrow = project_area_layout(600.0, 800.0, 2);
    assert_eq!(
        narrow[1].origin,
        Vec2::new(22.0, 376.5),
        "窄窗两个项目应上下排列"
    );
    assert!(
        project_area_layout(1280.0, 800.0, 0).is_empty(),
        "空工作空间不得虚构项目"
    );
}

/// 密集项目仍将完整画布等比放入极小内容区，不反向缩放。
/// 数据构造：600×600放10项目；项目高38.5、标题20、底边10，可用高8.5；原画布572×473。
/// 执行过程：计算项目网格 → 取得第一个项目的缩略内容矩形。
/// 预期结果：缩略高8.5，宽572×8.5/473；仍在外壳内，不出现负高或变形。
#[test]
fn desktop_dense_project_previews_never_invert_card_geometry() {
    use super::super::preview_canvas::project_canvas_rect;
    let area = project_area_layout(600.0, 600.0, 10)[0];
    let logical = super::super::desktop_scene_rect(600.0, 600.0).size;
    let fitted = project_canvas_rect(area, logical, 1.0);
    assert_eq!(fitted.size.y, 8.5, "应适配标题下方8.5px内容区");
    assert!(
        (fitted.size.x - 572.0 * 8.5 / 473.0).abs() < 0.001,
        "整个画布须等比缩小"
    );
    assert!(fitted.size.x > 0.0 && fitted.size.y > 0.0, "不允许负尺寸");
    assert!(
        fitted.origin.y >= area.origin.y && fitted.max().y < area.max().y,
        "缩略内容不得越出外壳"
    );
}
