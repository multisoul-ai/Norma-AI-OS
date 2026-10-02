use canvas::{layout::compute_project_region_layout, CanvasRect, Vec2};

/// 项目数量决定两行网格，奇数保留末尾空位。
///
/// 数据构造：可用区域 (14,36,1252,673)，间距 8；C=2/3/4/6/16，
///   格宽分别为 614、406.6667、303、199.3333、69.75，高始终 324.5。
/// 执行过程：逐个输入 1–8、12、31 项，检查首格、最后一格与一屏边界，再检查 0 项。
/// 预期结果：项目等大且按行填充，1项不铺满，奇数不伪造末尾项目；没有第三排。
#[test]
fn project_counts_reserve_two_rows_in_catalog_order() {
    let area = CanvasRect::new(14.0, 36.0, 1252.0, 673.0);
    // 独立规则表：(项目数, 每格宽, 末项列号, 末项行号)。
    for (count, width, last_col, last_row) in [
        (1, 614.0, 0, 0),
        (2, 614.0, 1, 0),
        (3, 614.0, 0, 1),
        (4, 614.0, 1, 1),
        (5, 406.66666, 1, 1),
        (6, 406.66666, 2, 1),
        (7, 303.0, 2, 1),
        (8, 303.0, 3, 1),
        (12, 199.33333, 5, 1),
        (31, 69.75, 14, 1),
    ] {
        let regions = compute_project_region_layout(count, area, 8.0);
        assert_eq!(regions.len(), count, "{count} 项不能添加假占位");
        assert_eq!(
            regions[0].origin,
            Vec2::new(22.0, 44.0),
            "{count} 项都从左上角开始"
        );
        assert!(
            (regions[0].size.x - width).abs() < 0.001,
            "{count} 项宽度不符合规则表"
        );
        assert_eq!(regions[0].size.y, 324.5, "{count} 项必须保留两行高度");
        let last = regions[count - 1];
        assert!(
            (last.origin.x - (22.0 + last_col as f32 * (width + 8.0))).abs() < 0.001,
            "末项列号错误: {count}"
        );
        assert_eq!(
            last.origin.y,
            44.0 + last_row as f32 * 332.5,
            "末项行号错误: {count}"
        );
        assert!(
            last.max().x <= 1258.001 && last.max().y <= 701.001,
            "不能超出内容区: {count}"
        );
        assert_ne!(regions[0].size, area.size, "总览单项目不能全屏");
    }
    assert!(
        compute_project_region_layout(0, area, 8.0).is_empty(),
        "空 Workspace 没有项目矩形"
    );
}

/// 窄窗及极小缩略图收缩间距，保持正尺寸且仍为两行。
///
/// 数据构造：10×6 区域放31项，16列；g=min(8,10/34,6/6)=5/17。
///   宽=(10-17×5/17)/16=0.3125；高=(6-15/17)/2=2.558824。
/// 执行过程：计算小区域网格，再传入零宽、负高及无穷宽。
/// 预期结果：31 项仍在一屏，两行等大；无效区域返回空列表，不产生负数或 NaN。
#[test]
fn project_regions_fit_tiny_and_invalid_viewports() {
    let small = compute_project_region_layout(31, CanvasRect::new(0.0, 0.0, 10.0, 6.0), 8.0);
    assert_eq!(small.len(), 31, "小区域不能丢项目");
    assert_eq!(small[0].size.x, 0.3125, "间距收缩后应保留半宽给项目");
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
        compute_project_region_layout(1, CanvasRect::new(0.0, 0.0, 0.0, 6.0), 8.0).is_empty(),
        "零宽没有合法格子"
    );
    assert!(
        compute_project_region_layout(1, CanvasRect::new(0.0, 0.0, 10.0, -1.0), 8.0).is_empty(),
        "负高没有合法格子"
    );
    assert!(
        compute_project_region_layout(1, CanvasRect::new(0.0, 0.0, f32::INFINITY, 6.0), 8.0)
            .is_empty(),
        "非有限区域不能传播"
    );
}
