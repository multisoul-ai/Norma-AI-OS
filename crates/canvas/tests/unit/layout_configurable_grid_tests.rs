use super::*;
use crate::geometry::Vec2;

fn approx_vec(actual: Vec2, expected: Vec2) -> bool {
    (actual.x - expected.x).abs() < 1e-4 && (actual.y - expected.y).abs() < 1e-4
}

/// 可配置网格按行向下：固定两列后，第 3、5 张卡片必须分别落入第二、第三行的第一列。
///
/// 数据构造（含关键数值的推导过程）：
///   node_ids = [11, 12, 13, 14, 15]，共 5 张卡片
///   track_count = 2，表示每行固定 2 列
///   fixed_card_width = 100，aspect_ratio = 2，因此 fixed_card_height = 100 / 2 = 50
///   gap = 10，safe_area = 0，因此首格 origin = (10,10)
///   id=13 index=2：row = 2 / 2 = 1，col = 2 % 2 = 0 → origin=(10,70)
///   id=15 index=4：row = 4 / 2 = 2，col = 4 % 2 = 0 → origin=(10,130)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 以 `RowFirst` 和固定尺寸配置计算 5 个网格 assignment。
///   2. 第 1、2 张按索引填入第一行的两列。
///   3. 第 3、4 张填入第二行；第 5 张继续向下累加到第三行。
///
/// 预期结果：
///   - 断言 A：id=13 存在且位于第二行第一列。
///   - 断言 B：id=15 存在且位于第三行第一列。
///   - 断言 C：所有卡片保持 100×50，不应因节点数从 2 增至 5 而缩放。
///   - 断言 D：id=15 不应错误地出现在第一行第三列。
#[test]
fn test_configurable_grid_row_first_grows_down_with_fixed_columns() {
    let assignments = compute_configurable_grid_layout(
        &[11, 12, 13, 14, 15],
        CanvasRect::new(0.0, 0.0, 800.0, 600.0),
        ConfigurableGridConfig {
            fill: GridFill::RowFirst,
            track_count: 2,
            sizing: GridCardSizing::Fixed,
            aspect_ratio: 2.0,
            fixed_card_width: 100.0,
            gap_canvas: 10.0,
            safe_area: LayoutInsets::ZERO,
        },
    );

    assert_eq!(
        assignments.len(),
        5,
        "5 张卡片应生成 5 个 assignment，缺失 assignment 会导致新增卡片无法落位"
    );
    assert!(
        approx_vec(assignments[2].bounds.origin, Vec2::new(10.0, 70.0)),
        "id=13（index=2）应位于第二行第一列 (10,70)，实际 {:?}",
        assignments[2].bounds.origin
    );
    assert!(
        approx_vec(assignments[4].bounds.origin, Vec2::new(10.0, 130.0)),
        "id=15（index=4）应位于第三行第一列 (10,130)，实际 {:?}",
        assignments[4].bounds.origin
    );
    assert_eq!(
        assignments[2].bounds.size,
        Vec2::new(100.0, 50.0),
        "固定尺寸下 id=13 必须保持 100×50，新增卡片不能压缩已有网格规格"
    );
    assert_eq!(
        assignments[4].bounds.size,
        Vec2::new(100.0, 50.0),
        "固定尺寸下 id=15 必须保持 100×50，不能因第五张卡片出现而缩放"
    );
    assert!(
        !approx_vec(assignments[4].bounds.origin, Vec2::new(230.0, 10.0)),
        "id=15 不应落在第一行第三列 (230,10)，RowFirst 必须向下而非突破列数"
    );
}

/// 可配置网格按列向右：固定两行后，第 3、5 张卡片必须分别落入第二、第三列的第一行。
///
/// 数据构造（含关键数值的推导过程）：
///   node_ids = [21, 22, 23, 24, 25]，共 5 张卡片
///   track_count = 2，表示每列固定 2 行
///   fixed_card_width = 100，aspect_ratio = 2，因此 fixed_card_height = 50
///   gap = 10，safe_area = 0，因此首格 origin = (10,10)
///   id=23 index=2：row = 2 % 2 = 0，col = 2 / 2 = 1 → origin=(120,10)
///   id=25 index=4：row = 4 % 2 = 0，col = 4 / 2 = 2 → origin=(230,10)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 以 `ColumnFirst` 和固定尺寸配置计算 5 个网格 assignment。
///   2. 第 1、2 张按索引填入第一列的两行。
///   3. 第 3、4 张填入第二列；第 5 张继续向右累加到第三列。
///
/// 预期结果：
///   - 断言 A：id=23 存在且位于第二列第一行。
///   - 断言 B：id=25 存在且位于第三列第一行。
///   - 断言 C：所有卡片保持 100×50，不应因节点数变化缩放。
///   - 断言 D：id=25 不应错误地落入第三行第一列。
#[test]
fn test_configurable_grid_column_first_grows_right_with_fixed_rows() {
    let assignments = compute_configurable_grid_layout(
        &[21, 22, 23, 24, 25],
        CanvasRect::new(0.0, 0.0, 800.0, 600.0),
        ConfigurableGridConfig {
            fill: GridFill::ColumnFirst,
            track_count: 2,
            sizing: GridCardSizing::Fixed,
            aspect_ratio: 2.0,
            fixed_card_width: 100.0,
            gap_canvas: 10.0,
            safe_area: LayoutInsets::ZERO,
        },
    );

    assert_eq!(
        assignments.len(),
        5,
        "5 张卡片应生成 5 个 assignment，固定两行时第五张仍必须有可用格子"
    );
    assert!(
        approx_vec(assignments[2].bounds.origin, Vec2::new(120.0, 10.0)),
        "id=23（index=2）应位于第二列第一行 (120,10)，实际 {:?}",
        assignments[2].bounds.origin
    );
    assert!(
        approx_vec(assignments[4].bounds.origin, Vec2::new(230.0, 10.0)),
        "id=25（index=4）应位于第三列第一行 (230,10)，实际 {:?}",
        assignments[4].bounds.origin
    );
    assert_eq!(
        assignments[4].bounds.size,
        Vec2::new(100.0, 50.0),
        "固定两行向右累加时 id=25 仍必须是 100×50，不能压缩已有卡片"
    );
    assert!(
        !approx_vec(assignments[4].bounds.origin, Vec2::new(10.0, 130.0)),
        "id=25 不应落入第三行第一列 (10,130)，ColumnFirst 必须向右而非突破行数"
    );
}

/// 适应可视区域：按行和按列布局都应平分当前可视窗口的宽与高，不能因保留长宽比留下大块空白。
///
/// 数据构造（含关键数值的推导过程）：
///   area = 500×300，safe_area = 0，gap = 10，track_count = 2，aspect_ratio = 2
///   宽格尺寸 = (500 - 10×(2+1)) / 2 = 235；高格尺寸 = (300 - 10×(2+1)) / 2 = 135
///   两种配置都传入 3 张卡片，因此实际均为 2 列×2 行；每张卡片应为 235×135
///   `aspect_ratio = 2` 仅供 Fixed 策略使用；FitViewport 不应为了保持比例把高度压到 117.5。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 以 `RowFirst + FitViewport` 计算三张卡片，按两列两行平分可视窗口。
///   2. 以 `ColumnFirst + FitViewport` 计算三张卡片，按两行两列平分可视窗口。
///   3. 比较两组第 1 与第 3 张的尺寸和第 3 张位置。
///
/// 预期结果：
///   - 断言 A：按行时卡片宽 235、高 135，第三张位于第二行第一列。
///   - 断言 B：按列时卡片宽 235、高 135，第三张位于第二列第一行。
///   - 断言 C：扩展到第三张后尺寸仍与首张相同，且网格填满可用高度。
///   - 断言 D：按列时第三张不应误落到第三行第一列。
#[test]
fn test_configurable_grid_fit_viewport_constrains_both_axes() {
    let row_first = compute_configurable_grid_layout(
        &[31, 32, 33],
        CanvasRect::new(0.0, 0.0, 500.0, 300.0),
        ConfigurableGridConfig {
            fill: GridFill::RowFirst,
            track_count: 2,
            sizing: GridCardSizing::FitViewport,
            aspect_ratio: 2.0,
            fixed_card_width: 100.0,
            gap_canvas: 10.0,
            safe_area: LayoutInsets::ZERO,
        },
    );
    let column_first = compute_configurable_grid_layout(
        &[41, 42, 43],
        CanvasRect::new(0.0, 0.0, 500.0, 300.0),
        ConfigurableGridConfig {
            fill: GridFill::ColumnFirst,
            track_count: 2,
            sizing: GridCardSizing::FitViewport,
            aspect_ratio: 2.0,
            fixed_card_width: 100.0,
            gap_canvas: 10.0,
            safe_area: LayoutInsets::ZERO,
        },
    );

    assert_eq!(
        row_first[0].bounds.size,
        Vec2::new(235.0, 135.0),
        "按行适应视区应以两列两行填满 500×300 的可视区域，首卡尺寸必须是 235×135"
    );
    assert!(
        approx_vec(row_first[2].bounds.origin, Vec2::new(10.0, 155.0)),
        "按行适应视区时第三张应向下位于第二行第一列 (10,155)，实际 {:?}",
        row_first[2].bounds.origin
    );
    assert_eq!(
        row_first[2].bounds.size, row_first[0].bounds.size,
        "按行新增第三张后不得改小第一张或第三张；增长方向不应改变卡片尺寸"
    );
    assert_eq!(
        column_first[0].bounds.size,
        Vec2::new(235.0, 135.0),
        "按列适应视区必须以两列两行填满可视区域，首卡尺寸必须是 235×135"
    );
    assert!(
        approx_vec(column_first[2].bounds.origin, Vec2::new(255.0, 10.0)),
        "按列适应视区时第三张应向右位于第二列第一行 (255,10)，实际 {:?}",
        column_first[2].bounds.origin
    );
    assert_eq!(
        column_first[2].bounds.size, column_first[0].bounds.size,
        "按列新增第三张后尺寸应与首张一致，且两行网格应填满当前可视高度"
    );
    assert!(
        !approx_vec(column_first[2].bounds.origin, Vec2::new(10.0, 300.0)),
        "按列适应视区时第三张不应落入第三行第一列 (10,300)"
    );
}

/// 可配置网格容纳新增卡片：固定两列达到三行后必须等比例缩小，所有卡片仍留在当前可视区域内。
///
/// 数据构造（含关键数值的推导过程）：
///   area = 300×150，node_ids = [51,52,53,54,55,56]，共 6 张
///   track_count = 2 → rows = ceil(6 / 2) = 3；gap = 10；aspect_ratio = 2
///   水平上限 = (300 - 10×3) / 2 = 135 宽；垂直上限 = (150 - 10×4) / 3 = 110/3 高
///   为保持 2:1，垂直给出的宽上限 = (110/3)×2 = 220/3 ≈ 73.33
///   fixed_card_width = 100，因此最终宽 = min(100,135,220/3) = 220/3，高 = 110/3
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 以 `RowFirst + Fixed` 计算 6 张卡片，数量固定为每行两列。
///   2. 根据 3 行和当前 150 高的可视区域收缩卡片，而不是向下扩张画布。
///   3. 读取首卡和最后一张卡片的尺寸及右下角坐标。
///
/// 预期结果：
///   - 断言 A：首卡宽高为 220/3×110/3，证明固定尺寸在空间不足时会等比例缩小。
///   - 断言 B：最后一张仍在第三行第二列，且右下角不超过可视区域。
///   - 断言 C：最后一张不得保留 100×50，否则第三行会溢出当前窗口。
#[test]
fn test_configurable_grid_shrinks_fixed_cards_to_keep_all_rows_inside_viewport() {
    let assignments = compute_configurable_grid_layout(
        &[51, 52, 53, 54, 55, 56],
        CanvasRect::new(0.0, 0.0, 300.0, 150.0),
        ConfigurableGridConfig {
            fill: GridFill::RowFirst,
            track_count: 2,
            sizing: GridCardSizing::Fixed,
            aspect_ratio: 2.0,
            fixed_card_width: 100.0,
            gap_canvas: 10.0,
            safe_area: LayoutInsets::ZERO,
        },
    );

    assert!(
        approx_vec(
            assignments[0].bounds.size,
            Vec2::new(220.0 / 3.0, 110.0 / 3.0)
        ),
        "6 张卡片占三行时首卡必须缩至 220/3×110/3 以保持在 300×150 视区内，实际 {:?}",
        assignments[0].bounds.size
    );
    assert!(
        approx_vec(
            assignments[5].bounds.origin,
            Vec2::new(280.0 / 3.0, 310.0 / 3.0)
        ),
        "第 6 张必须位于第三行第二列，实际 {:?}",
        assignments[5].bounds.origin
    );
    assert!(
        assignments[5].bounds.max().x <= 290.001 && assignments[5].bounds.max().y <= 140.001,
        "第 6 张的右下角必须保留 10px 边距并位于当前 300×150 视区内，实际 {:?}",
        assignments[5].bounds.max()
    );
    assert_ne!(
        assignments[5].bounds.size,
        Vec2::new(100.0, 50.0),
        "空间不足时第 6 张不得保留固定 100×50，否则布局会向窗口外溢出"
    );
}
