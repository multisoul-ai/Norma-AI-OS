use super::*;
use crate::canvas::layout_strategy::CanvasLayoutStrategy;
use canvas::{compute_configurable_grid_layout, Vec2};

/// 内嵌项目由外层预留窗口控件和 Dock，内部卡片从局部画布边缘开始排版。
///
/// 数据构造（含关键数值的推导过程）：
///   窗口 1280×800；外层左右各 14、顶部 52+12=64、底部 91，局部尺寸 1252×645；
///   内部 gap=8，三列卡片宽=(1252-4×8)/3，高=645-2×8=629。
/// 执行过程（逐步说明系统如何处理）：
///   1. 计算项目局部尺寸 → 扣除外层标题和 Dock。
///   2. 选择内嵌安全边距并布局两张卡片 → 内部仅保留网格间距。
///   3. 将第一张卡片顶部转换为窗口坐标，并与独立画布的安全边距比较。
///
/// 预期结果：
///   - 正断言：第一张从局部 (8,8) 开始；全局顶部64+8=72在52px标题下方。
///   - 正断言：第二张底部637不超过645；独立画布保留86px顶部边距。
///   - 负断言：内嵌布局不重复使用独立画布的安全边距，卡片不能被底部裁掉。
#[test]
fn desktop_embedded_layout_does_not_reserve_chrome_twice() {
    let scene = super::super::desktop_scene_rect(1280.0, 800.0);
    assert_eq!(
        scene.size,
        Vec2::new(1252.0, 645.0),
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
    assert!(
        scene.origin.y + rects[0].bounds.origin.y > 52.0,
        "第一张卡片必须完整处于顶部标题栏下方"
    );
    assert_eq!(
        rects[1].bounds.max().y,
        637.0,
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

/// 项目区域保留两行空位，以16:9比例和大留白呈现。
///
/// 数据构造：1280×800，左右7.5%=96，顶部max(6%×800,52+12)=64，底部max(10%,91)=91；
///   区域1088×645，列距2.5%×1280=32、行距8%×800=64；
///   每格高(645-64)/2=290.5，宽290.5×16/9≈516.44446，首列x≈107.55554。
///   600×800时左右45、列距15，格宽247.5、高139.21875，首行y=64+(645-342.4375)/2=215.28125。
/// 执行过程：分别排列 0、1、2、3、4 项与 600px 窄窗的 2 项。
/// 预期结果：完整网格居中，第一格固定左上槽位，第三格换到左下；窄窗保持横向比例，无虚构项目。
#[test]
fn desktop_project_regions_reserve_two_rows() {
    let one = project_area_layout(1280.0, 800.0, 1);
    assert_eq!(
        one,
        vec![CanvasRect::new(107.55554, 64.0, 516.44446, 290.5)],
        "单项目只占左上角"
    );
    let two = project_area_layout(1280.0, 800.0, 2);
    assert_eq!(
        two[1],
        CanvasRect::new(656.0, 64.0, 516.44446, 290.5),
        "第二项仍在上排"
    );
    let three = project_area_layout(1280.0, 800.0, 3);
    assert_eq!(
        three[2],
        CanvasRect::new(107.55554, 418.5, 516.44446, 290.5),
        "第三项进入左下角"
    );
    let four = project_area_layout(1280.0, 800.0, 4);
    assert_eq!(
        four[3],
        CanvasRect::new(656.0, 418.5, 516.44446, 290.5),
        "第四项填右下角"
    );
    let narrow = project_area_layout(600.0, 800.0, 2);
    assert_eq!(
        narrow[1],
        CanvasRect::new(307.5, 215.28125, 247.5, 139.21875),
        "窄窗依旧两列"
    );
    assert_ne!(one[0].size.y, 657.0, "单项目总览不应铺满整个画布");
    assert!(
        project_area_layout(1280.0, 800.0, 0).is_empty(),
        "不绘制空项目占位"
    );
}

/// 项目外壳使用紧凑标题和内边距，真实画布保持等比。
///
/// 数据构造（含关键数值的推导过程）：
///   600×600、10项目，5列宽(510-4×15)/5=90，高90×9/16=50.625；
///   chrome=50.625/56=0.904017857，标题28×chrome=25.3125，内边距8×chrome≈7.232143；
///   预览可用高50.625-36×chrome≈18.080357，画布572×445（600-64-91）按高度适配，宽≈23.240369。
/// 执行过程（逐步说明系统如何处理）：
///   1. 排列10个项目，取首项目的标题与画布内容矩形。
///   2. 按项目画布的完整尺寸等比缩小 → 内容适配标题下的可用高度。
///   3. 在8×4的极小外壳中再次适配 → 检查内容边界。
///
/// 预期结果：
///   - 正断言：标题与边距按比例收缩，画布完整等比缩小。
///   - 负断言：极小外壳不溢出，内容宽高不能为负或倒转。
#[test]
fn desktop_dense_project_previews_never_invert_card_geometry() {
    use super::super::preview_canvas::{project_canvas_rect, project_stage_rect};
    let area = project_area_layout(600.0, 600.0, 10)[0];
    let logical = super::super::desktop_scene_rect(600.0, 600.0).size;
    let stage = project_stage_rect(area.size, 1.0);
    assert!(
        (stage.origin.x - 7.232143).abs() < 0.001,
        "矮缩略的内边距应随chrome等比缩小"
    );
    assert_eq!(
        stage.origin.y, 25.3125,
        "标题在矮缩略内应等比收缩而非挤出内容"
    );
    let fitted = project_canvas_rect(area, logical, 1.0);
    assert!(
        (fitted.size.y - 18.080357).abs() < 0.001,
        "预览使用完整可用高"
    );
    assert!(
        (fitted.size.x - 23.240_37).abs() < 0.001,
        "真实画布必须等比缩小"
    );
    let tiny = CanvasRect::new(0.0, 0.0, 8.0, 4.0);
    let fitted = project_canvas_rect(tiny, logical, 1.0);
    assert!(
        fitted.size.x > 0.0 && fitted.size.y > 0.0,
        "极小外壳不能倒转内容"
    );
    assert!(
        fitted.origin.y > 0.0 && fitted.max().y < 4.0 && fitted.max().x < 8.0,
        "标题与内容均留在小外壳内"
    );
}
