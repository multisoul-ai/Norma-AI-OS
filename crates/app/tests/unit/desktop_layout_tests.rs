use super::*;
use crate::canvas::layout_strategy::CanvasLayoutStrategy;
use canvas::{compute_configurable_grid_layout, Vec2};

/// 只有一个工作空间时，主卡使用整行宽度，避免右侧留下未使用的空白列。
///
/// 数据构造（含关键数值的推导过程）：
///   1280×800 窗口；宽屏左右边距各 54，内容宽 = 1280 - 2×54 = 1172。
///   仅有 1 个工作空间，右侧没有需要预留位置的卡片。
/// 执行过程（逐步说明系统如何处理）：
///   1. 计算首页布局 → 返回唯一一张主卡。
///   2. 对比卡片右边界与内容区右边界。
/// 预期结果：
///   - 正断言：主卡从 x=54 延伸到 x=1226，占满内容区。
///   - 负断言：不会沿用多工作空间布局的 62% 列宽。
#[test]
fn desktop_single_workspace_fills_home_row() {
    let cards = home_layout(1280.0, 800.0, 1);
    assert_eq!(cards.len(), 1, "单工作空间首页应仅有一张卡片");
    assert_eq!(cards[0].origin.x, 54.0, "唯一主卡应从内容区左边界开始");
    assert_eq!(cards[0].size.x, 1172.0, "唯一主卡应占满全部可用宽度");
    assert_eq!(cards[0].max().x, 1226.0, "主卡右边界应与内容区右边界一致");
    assert_ne!(cards[0].size.x, 719.2, "单卡不能沿用多卡的 62% 列宽");
}

/// 首页主卡预览占满预览底板，六个项目在底板内紧凑排成两行。
///
/// 数据构造（含关键数值的推导过程）：
///   预览底板 = 800×360；四周内边距 8。
///   六项原布局 = 3列×2行，每项 400×225，源边界为 1200×450。
///   适配比例 = min((800-16)/1200, (360-16)/450) = 784/1200 ≈ 0.6533。
///   适配后宽 784、高 294；上下余量 = (360-294)/2 = 33。
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 800×360 的局部预览中计算六项目缩略图位置。
/// 预期结果：
///   - 首项和末项分别靠近左右边缘，六项全部处于底板内。
///   - 不能退回只占中间约 60% 宽度的旧排版。
#[test]
fn desktop_home_project_previews_fill_featured_panel() {
    let source = (0..6)
        .map(|index| {
            CanvasRect::new(
                (index % 3) as f32 * 400.0,
                (index / 3) as f32 * 225.0,
                400.0,
                225.0,
            )
        })
        .collect::<Vec<_>>();
    let rects = home_project_preview_layout(Vec2::new(800.0, 360.0), &source);
    assert_eq!(rects.len(), 6, "六个项目都应有缩略图，不能丢项");
    assert!(
        (rects[0].origin.x - 8.0).abs() < 0.01 && (rects[0].origin.y - 33.0).abs() < 0.01,
        "首项应靠近底板左边缘，并在上下居中后从 y=33 开始"
    );
    assert!(
        (rects[5].max().x - 792.0).abs() < 0.01 && (rects[5].max().y - 327.0).abs() < 0.01,
        "末项应到达右下方，且保留 8px 右边距和 33px 下边距"
    );
    assert!(
        rects[5].max().x <= 800.0 && rects[5].max().y <= 360.0,
        "末项不能从预览底板溢出"
    );
    assert!(
        rects[5].max().x > 800.0 * 0.9,
        "六项必须使用超过 90% 的可用宽度，不能缩在中间"
    );
}

/// 五项目总览增大预览卡片，收紧侧边与行列留白，同时保留标题栏和底部Dock。
///
/// 数据构造：1280×800窗口、5个项目；左右各24px，列距16px，行距24px；
///   三列卡片宽=(1280-2×24-2×16)/3=400，高400×9/16=225；
///   内容高800-64-91=645，网格高2×225+24=474，首行y=64+(645-474)/2=149.5。
/// 执行过程：计算五个项目的总览矩形，检查首项、右上项和下排末项的位置与尺寸。
/// 预期结果：卡片400×225，比原341.3333×192面积增大约37%；两行三列不变，
///   所有项目在标题与Dock之间，无虚构占位，也不拉伸画布比例。
#[test]
fn desktop_five_project_cards_use_larger_previews_and_compact_gaps() {
    let cards = project_area_layout(1280.0, 800.0, 5);
    assert_eq!(cards.len(), 5, "五项目应只绘制五个卡片");
    assert_eq!(
        cards[0],
        CanvasRect::new(24.0, 149.5, 400.0, 225.0),
        "首项目应按更大的预览尺寸居中排布"
    );
    assert_eq!(
        cards[2],
        CanvasRect::new(856.0, 149.5, 400.0, 225.0),
        "上排三列应使用16px间距并保留右侧24px留白"
    );
    assert_eq!(
        cards[4],
        CanvasRect::new(440.0, 398.5, 400.0, 225.0),
        "下排应使用24px行距，不能添加第六个占位项目"
    );
    assert!(cards[0].origin.y >= 64.0, "项目卡片不能遮住标题栏");
    assert!(cards[4].max().y <= 709.0, "项目卡片不能遮住底部Dock");
    assert!(
        cards[0].size.x * cards[0].size.y > 89_000.0,
        "预览面积应明显大于旧版65536px²，而非只移动卡片"
    );
}

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

/// 三个以上项目保留两行网格，以16:9比例和紧凑留白呈现。
///
/// 数据构造：1280×800，左右24，顶部max(6%×800,52+12)=64，底部max(10%,91)=91；
///   区域1232×645，列距16、行距24；每格高(645-24)/2=310.5，宽310.5×16/9=552，
///   首列x=24+(1232-2×552-16)/2=80。
/// 执行过程：分别排列 0、3、4 项。
/// 预期结果：完整网格居中，第三格换到左下；无虚构项目。
#[test]
fn desktop_project_regions_reserve_two_rows() {
    let three = project_area_layout(1280.0, 800.0, 3);
    assert_eq!(
        three[2],
        CanvasRect::new(80.0, 398.5, 552.0, 310.5),
        "第三项进入左下角"
    );
    let four = project_area_layout(1280.0, 800.0, 4);
    assert_eq!(
        four[3],
        CanvasRect::new(648.0, 398.5, 552.0, 310.5),
        "第四项填右下角"
    );
    assert!(
        project_area_layout(1280.0, 800.0, 0).is_empty(),
        "不绘制空项目占位"
    );
}

/// 一两个项目只占一行，预览使用可用高度并在工作空间卡片中居中。
///
/// 数据构造（含关键数值的推导过程）：
///   1280×800 视口 → 内容区 x=24、y=64、宽=1232、高=800-64-91=645。
///   两项：宽=(1232-16)/2=608，高=608×9/16=342，居中 y=64+(645-342)/2=215.5。
///   单项：宽=min(1232,645×16/9)=1146.667，高=645，居中 x≈66.667。
///
/// 执行过程：分别计算一个和两个项目的总览布局，读取首项与末项矩形。
///
/// 预期结果：
///   - 正断言：两项明显大于旧版 552×310.5，并在纵向居中；单项占满可用高度。
///   - 负断言：没有第二行空位，两个项目也不能重叠或越出内容区。
#[test]
fn desktop_sparse_project_previews_fill_the_available_row() {
    let one = project_area_layout(1280.0, 800.0, 1);
    let two = project_area_layout(1280.0, 800.0, 2);
    assert_eq!(one.len(), 1, "单项目不能生成第二行的空卡片");
    assert_eq!(two.len(), 2, "两项目必须保留两张预览");
    assert!(
        (one[0].origin.x - 66.6667).abs() < 0.01 && (one[0].size.x - 1146.6667).abs() < 0.01,
        "单项目应横向居中并使用可用高度"
    );
    assert_eq!(one[0].origin.y, 64.0, "单项目应从内容区顶部开始");
    assert_eq!(one[0].size.y, 645.0, "单项目应占满内容区高度");
    assert_eq!(
        two[0],
        CanvasRect::new(24.0, 215.5, 608.0, 342.0),
        "首项目应放大并在唯一一行中居中"
    );
    assert_eq!(
        two[1],
        CanvasRect::new(648.0, 215.5, 608.0, 342.0),
        "第二项目应与首项等大且不重叠"
    );
    assert_ne!(two[0].origin.y, 64.0, "两项不能继续占用预留第二行的旧布局");
    assert!(two[1].max().x <= 1256.0, "右侧项目不能超出内容区边界");
}

/// 项目外壳使用紧凑标题和内边距，真实画布保持等比。
///
/// 数据构造（含关键数值的推导过程）：
///   600×600、10项目，5列宽(552-4×16)/5=97.6，高97.6×9/16=54.9；
///   chrome=54.9/56≈0.980357，标题28×chrome=27.45，内边距8×chrome≈7.842857；
///   预览可用高54.9-36×chrome≈19.607143，画布572×445（600-64-91）按高度适配，宽≈25.202889。
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
        (stage.origin.x - 7.842857).abs() < 0.001,
        "矮缩略的内边距应随chrome等比缩小"
    );
    assert!(
        (stage.origin.y - 27.45).abs() < 0.001,
        "标题在矮缩略内应等比收缩而非挤出内容"
    );
    let fitted = project_canvas_rect(area, logical, 1.0);
    assert!(
        (fitted.size.y - 19.607143).abs() < 0.001,
        "预览使用完整可用高"
    );
    assert!(
        (fitted.size.x - 25.202_89).abs() < 0.001,
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
