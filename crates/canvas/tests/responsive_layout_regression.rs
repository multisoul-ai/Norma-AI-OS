use canvas::{
    compute_responsive_grid_layout, CanvasRect, LayoutInsets, ResponsiveGridConfig,
    DEFAULT_CARD_ASPECT_RATIO,
};

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

fn assert_rect(actual: CanvasRect, expected: CanvasRect, label: &str) {
    assert!(
        approx(actual.origin.x, expected.origin.x),
        "{label} origin.x should be {}, got {}",
        expected.origin.x,
        actual.origin.x
    );
    assert!(
        approx(actual.origin.y, expected.origin.y),
        "{label} origin.y should be {}, got {}",
        expected.origin.y,
        actual.origin.y
    );
    assert!(
        approx(actual.size.x, expected.size.x),
        "{label} width should be {}, got {}",
        expected.size.x,
        actual.size.x
    );
    assert!(
        approx(actual.size.y, expected.size.y),
        "{label} height should be {}, got {}",
        expected.size.y,
        actual.size.y
    );
}

fn screenshot_viewport_config() -> ResponsiveGridConfig {
    ResponsiveGridConfig {
        max_columns: 3,
        min_column_width: 320.0,
        aspect_ratio: DEFAULT_CARD_ASPECT_RATIO,
        gap_canvas: 16.0,
        safe_area: LayoutInsets::new(96.0, 86.0, 32.0, 84.0),
    }
}

/// R-1: 截图窗口下单卡、双卡、六卡都按可用高度填满行。
///
/// 数据构造（含关键数值推导）：
///   viewport = 1512x945 logical px（3024x1890 截图按 2x 缩放）
///   safe area = left 96 + top 86 + right 32 + bottom 84
///   usable_width  = 1512 - 96 - 32 = 1384
///   usable_height =  945 - 86 - 84 = 775
///   gap = 16
///   单卡：columns=1, rows=1, width=1384 - 16*2 = 1352, height=775 - 16*2 = 743
///   双卡：columns=2, rows=1, width=(1384 - 16*3)/2 = 668, height=743
///   六卡：columns=3, rows=2, width=(1384 - 16*4)/3 = 440, height=(775 - 16*3)/2 = 363.5
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 对 [1] 执行响应式布局 → 单卡位于安全区内第一行，底部保留 16px 内部 gap
///   2. 对 [1,2] 执行响应式布局 → 两卡同一行，沿用同一行高 743
///   3. 对 [1,2,3,4,5,6] 执行响应式布局 → 三列两行，总高度填满安全区
///
/// 预期结果：
///   - 正断言：单卡高度为 743，max.y=845，不超过安全区底部 861
///   - 正断言：双卡高度同为 743，第二卡 x=796
///   - 正断言：六卡第二行 y=481.5，高度 363.5
///   - 负断言：双卡不应保留旧宽高比高度约 526.06，六卡不应保留旧宽高比高度约 346.5
#[test]
fn test_screenshot_viewport_responsive_cards_fill_available_row_height() {
    let area = CanvasRect::new(0.0, 0.0, 1512.0, 945.0);
    let config = screenshot_viewport_config();

    let single = compute_responsive_grid_layout(&[1], area, config);
    let two = compute_responsive_grid_layout(&[1, 2], area, config);
    let six = compute_responsive_grid_layout(&[1, 2, 3, 4, 5, 6], area, config);

    assert_eq!(single.len(), 1, "单卡布局应返回 1 个 assignment");
    assert_eq!(two.len(), 2, "双卡布局应返回 2 个 assignment");
    assert_eq!(six.len(), 6, "六卡布局应返回 6 个 assignment");

    assert_rect(
        single[0].bounds,
        CanvasRect::new(112.0, 102.0, 1352.0, 743.0),
        "single card",
    );
    assert!(
        single[0].bounds.max().y <= 861.0,
        "单卡底部应在安全区底部 861 内，实际 {}",
        single[0].bounds.max().y
    );

    assert_rect(
        two[0].bounds,
        CanvasRect::new(112.0, 102.0, 668.0, 743.0),
        "two card first",
    );
    assert_rect(
        two[1].bounds,
        CanvasRect::new(796.0, 102.0, 668.0, 743.0),
        "two card second",
    );
    assert!(
        (two[0].bounds.size.y - 526.05994).abs() > 1.0,
        "双卡高度不应停留在旧宽高比高度约 526.06，实际 {}",
        two[0].bounds.size.y
    );

    assert_rect(
        six[0].bounds,
        CanvasRect::new(112.0, 102.0, 440.0, 363.5),
        "six card first",
    );
    assert_rect(
        six[3].bounds,
        CanvasRect::new(112.0, 481.5, 440.0, 363.5),
        "six card fourth",
    );
    assert!(
        (six[0].bounds.size.y - 346.5).abs() > 1.0,
        "六卡高度不应停留在旧宽高比高度约 346.5，实际 {}",
        six[0].bounds.size.y
    );
    assert!(
        approx(six[3].bounds.max().y + 16.0, 861.0),
        "第二行底部加底部 gap 应到安全区底部 861，实际 {}",
        six[3].bounds.max().y + 16.0
    );
}
