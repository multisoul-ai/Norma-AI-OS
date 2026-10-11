use canvas::{layout::compute_project_region_layout, CanvasRect, Vec2};

/// 项目数量决定两行16:9网格，奇数保留末尾空位，完整网格居中。
///
/// 数据构造：可用区域 (96,48,1088,661)，列距32、行距64；C=2/3/4/6/16，
///   格宽=(1088-(C-1)×32)/C，依次528、341.3333、248、154.6667、38；
///   高=宽×9/16，首行y=48+(661-2×高-64)/2，第二行统一为410.5。
/// 执行过程：逐个输入 1–8、12、31 项，检查首格、最后一格与一屏边界，再检查 0 项。
/// 预期结果：项目等大且按行填充，1项保留左上槽位，数量再多也不拉成竖条；没有第三排或假项目。
#[test]
fn project_counts_reserve_two_rows_in_catalog_order() {
    let area = CanvasRect::new(96.0, 48.0, 1088.0, 661.0);
    let gap = Vec2::new(32.0, 64.0);
    // 独立规则表：(项目数, 每格宽, 每格高, 首行y, 末项列号, 末项行号)。
    for (count, width, height, top, last_col, last_row) in [
        (1, 528.0, 297.0, 49.5, 0, 0),
        (2, 528.0, 297.0, 49.5, 1, 0),
        (3, 528.0, 297.0, 49.5, 0, 1),
        (4, 528.0, 297.0, 49.5, 1, 1),
        (5, 341.33333, 192.0, 154.5, 1, 1),
        (6, 341.33333, 192.0, 154.5, 2, 1),
        (7, 248.0, 139.5, 207.0, 2, 1),
        (8, 248.0, 139.5, 207.0, 3, 1),
        (12, 154.66667, 87.0, 259.5, 5, 1),
        (31, 38.0, 21.375, 325.125, 14, 1),
    ] {
        let regions = compute_project_region_layout(count, area, gap);
        assert_eq!(regions.len(), count, "{count} 项不能添加假占位");
        assert!(
            (regions[0].origin.x - 96.0).abs() < 0.001 && (regions[0].origin.y - top).abs() < 0.001,
            "{count} 项都从居中网格的左上槽位开始"
        );
        assert!(
            (regions[0].size.x - width).abs() < 0.001,
            "{count} 项宽度不符合规则表"
        );
        assert!(
            (regions[0].size.y - height).abs() < 0.001,
            "{count} 项必须维持16:9高度"
        );
        let last = regions[count - 1];
        assert!(
            (last.origin.x - (96.0 + last_col as f32 * (width + 32.0))).abs() < 0.001,
            "末项列号错误: {count}"
        );
        assert_eq!(
            last.origin.y,
            top + last_row as f32 * (height + 64.0),
            "末项行号错误: {count}"
        );
        assert!(
            last.max().x <= 1184.001 && last.max().y <= 709.001,
            "不能超出内容区: {count}"
        );
        assert_ne!(regions[0].size, area.size, "总览单项目不能全屏");
    }
    assert!(
        compute_project_region_layout(0, area, gap).is_empty(),
        "空 Workspace 没有项目矩形"
    );
}

/// 窄窗及极小缩略图收缩间距，保持正尺寸且仍为两行。
///
/// 数据构造：10×6 区域放31项，16列；gx=min(32,10/30)=1/3，gy=min(64,6/2)=3。
///   宽=(10-15×1/3)/16=0.3125，高=0.3125×9/16=0.17578125。
/// 执行过程：计算小区域网格，再传入零宽、负高及无穷宽。
/// 预期结果：31 项仍在一屏，两行等大；无效区域返回空列表，不产生负数或 NaN。
#[test]
fn project_regions_fit_tiny_and_invalid_viewports() {
    let gap = Vec2::new(32.0, 64.0);
    let small = compute_project_region_layout(31, CanvasRect::new(0.0, 0.0, 10.0, 6.0), gap);
    assert_eq!(small.len(), 31, "小区域不能丢项目");
    assert_eq!(small[0].size.x, 0.3125, "间距收缩后应保留半宽给项目");
    assert_eq!(small[0].size.y, 0.17578125, "极小缩略也应保留16:9比例");
    assert!(
        small[30].max().x < 10.0 && small[30].max().y < 6.0,
        "末项不能溢出"
    );
    assert!(
        small
            .iter()
            .all(|rect| rect.size.x > 0.0 && rect.size.y > 0.0),
        "小区域不能倒转矩形"
    );
    assert!(
        compute_project_region_layout(1, CanvasRect::new(0.0, 0.0, 0.0, 6.0), gap).is_empty(),
        "零宽没有合法格子"
    );
    assert!(
        compute_project_region_layout(1, CanvasRect::new(0.0, 0.0, 10.0, -1.0), gap).is_empty(),
        "负高没有合法格子"
    );
    assert!(
        compute_project_region_layout(1, CanvasRect::new(0.0, 0.0, f32::INFINITY, 6.0), gap)
            .is_empty(),
        "非有限区域不能传播"
    );
}

/// 总览两行网格的纵向移动目标：同列换行、底行缺格吸附、行边界不环绕。
///
/// 数据构造：5 项 → 3 列（行 0 为索引 0/1/2，行 1 为 3/4）；4 项 → 2 列；
///   2 项与 1 项都在单行（columns ≥ count 时没有第二行）；0 项为空集。
/// 执行过程：从行 0 各列 down、行 1 各项 up、行边界双向、无选中双向、越界索引。
/// 预期结果：down 落到同列底行（底行缺格时吸附到末项），up 回到同列顶行；
///   顶行 up 与底行 down 不环绕；无选中时 down→首项、up→末项；单行/空集/越界无目标。
#[test]
fn overview_row_target_swaps_rows_in_the_same_column() {
    use canvas::layout::overview_row_target;
    // 5 项：行 0 = 0,1,2；行 1 = 3,4（3 列）。
    assert_eq!(
        overview_row_target(5, Some(0), false),
        Some(3),
        "down 应回到同列底行首项"
    );
    assert_eq!(
        overview_row_target(5, Some(1), false),
        Some(4),
        "down 应回到同列底行末项"
    );
    assert_eq!(
        overview_row_target(5, Some(2), false),
        Some(4),
        "底行缺格时应吸附到末项而不是虚构格子"
    );
    assert_eq!(
        overview_row_target(5, Some(3), true),
        Some(0),
        "up 应回到同列顶行"
    );
    assert_eq!(
        overview_row_target(5, Some(4), true),
        Some(1),
        "up 应回到同列顶行"
    );
    assert_eq!(
        overview_row_target(5, Some(0), true),
        None,
        "顶行 up 不环绕到底行"
    );
    assert_eq!(
        overview_row_target(5, Some(4), false),
        None,
        "底行 down 不环绕回顶行"
    );
    assert_eq!(
        overview_row_target(5, None, false),
        Some(0),
        "无选中时 down 从首项开始"
    );
    assert_eq!(
        overview_row_target(5, None, true),
        Some(4),
        "无选中时 up 从末项开始"
    );
    // 4 项：2 列，两行各 2 项。
    assert_eq!(
        overview_row_target(4, Some(1), false),
        Some(3),
        "4 项 down 同列换行"
    );
    assert_eq!(
        overview_row_target(4, Some(3), true),
        Some(1),
        "4 项 up 同列换行"
    );
    // 单行布局（columns ≥ count）、空集与越界索引都没有纵向目标。
    assert_eq!(
        overview_row_target(2, Some(0), false),
        None,
        "两项并排单行时没有可换的行"
    );
    assert_eq!(overview_row_target(1, None, true), None, "单项没有纵向目标");
    assert_eq!(
        overview_row_target(0, Some(0), false),
        None,
        "空集不能产生虚构目标"
    );
    assert_eq!(
        overview_row_target(5, Some(9), false),
        None,
        "越界索引没有目标"
    );
}

/// 矮宽窗口按高度等比缩小整组项目，单项目仍保留完整网格的左上槽位。
///
/// 数据构造：1024×352区域，列距32、行距64；每行高=(352-64)/2=144，宽=144×16/9=256；
///   网格宽=2×256+32=544，左侧留白=(1024-544)/2=240，第二行y=144+64=208。
/// 执行过程：计算4项目和1项目，比较左右两列、第二行底部及保留槽位的位置。
/// 预期结果：高度约束时两行仍一屏且16:9；不拉宽到窗口边缘，不把单个项目挪到窗口正中。
#[test]
fn project_regions_fit_height_without_stretching_or_recentering_occupied_slots() {
    let area = CanvasRect::new(0.0, 0.0, 1024.0, 352.0);
    let gap = Vec2::new(32.0, 64.0);
    let four = compute_project_region_layout(4, area, gap);
    assert_eq!(
        four[0],
        CanvasRect::new(240.0, 0.0, 256.0, 144.0),
        "矮窗按高度约束后整组居中"
    );
    assert_eq!(
        four[3],
        CanvasRect::new(528.0, 208.0, 256.0, 144.0),
        "右下项目应停在352px底边内"
    );
    assert_ne!(four[0].origin.x, 0.0, "高度受限时不能拉伸项目以填满宽度");
    let one = compute_project_region_layout(1, area, gap);
    assert_eq!(one.len(), 1, "保留槽位不能生成假项目");
    assert_eq!(one[0], four[0], "单项目仍位于完整2×2网格的左上槽位");
}
