use super::*;
use crate::geometry::Vec2;

fn approx_vec(a: Vec2, b: Vec2) -> bool {
    (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4
}

fn approx_rect(a: CanvasRect, b: CanvasRect) -> bool {
    approx_vec(a.origin, b.origin) && approx_vec(a.size, b.size)
}

/// 响应式编排：空节点列表不会因为安全区或列宽配置生成任何 assignment。
///
/// 数据构造（含关键数值推导）：
///   ordered_ids=[]
///   area = origin(0,0) size(1280x800)
///   safe_area = left(96)+top(86)+right(32)+bottom(84)
///   usable_width = 1280 - 96 - 32 = 1152，但节点数为 0，不应计算列宽
///
/// 执行过程：
///   1. compute_responsive_grid_layout([], area, config)
///   2. 入口看到 ordered_ids 为空 → 返回空 Vec
///
/// 预期结果：
///   - 正断言：assignments.len() == 0
///   - 负断言：结果不应包含 node_id=1
#[test]
fn test_responsive_grid_empty_ids_returns_no_assignments() {
    let assignments = compute_responsive_grid_layout(
        &[],
        CanvasRect::new(0.0, 0.0, 1280.0, 800.0),
        ResponsiveGridConfig {
            max_columns: 3,
            min_column_width: 320.0,
            aspect_ratio: 16.0 / 10.0,
            gap_canvas: 16.0,
            safe_area: LayoutInsets::new(96.0, 86.0, 32.0, 84.0),
        },
    );

    assert_eq!(
        assignments.len(),
        0,
        "空节点响应式布局不应生成任何 assignment"
    );
    assert!(
        !assignments.iter().any(|assignment| assignment.node_id == 1),
        "空节点响应式布局不应包含 node_id=1"
    );
}

/// 响应式编排：窄宽度下即使有三个节点也退回一列。
///
/// 数据构造（含关键数值推导）：
///   area = origin(0,0) size(420x700)
///   safe_area = 0 → usable_width = 420
///   gap = 16, min_column_width = 320
///   candidate 2 宽度 = (420 - 16*(2+1)) / 2 = 186 < 320 → 不可用
///   columns = 1
///   tile_width = 420 - 16*(1+1) = 388
///   tile_height = 388 / 1.6 = 242.5
///
/// 执行过程：
///   1. id=1 分到 row=0,col=0 → origin=(16,16)
///   2. id=2 分到 row=1,col=0 → y=16+242.5+16=274.5
///   3. id=3 分到 row=2,col=0
///
/// 预期结果：
///   - 正断言：id=1/id=2 均为一列宽 388
///   - 正断言：id=2 位于第二行第一列
///   - 负断言：id=2 不应位于两列布局的 x=218
#[test]
fn test_responsive_grid_narrow_area_uses_one_column() {
    let assignments = compute_responsive_grid_layout(
        &[1, 2, 3],
        CanvasRect::new(0.0, 0.0, 420.0, 700.0),
        ResponsiveGridConfig {
            max_columns: 3,
            min_column_width: 320.0,
            aspect_ratio: 16.0 / 10.0,
            gap_canvas: 16.0,
            safe_area: LayoutInsets::ZERO,
        },
    );

    assert_eq!(
        assignments.len(),
        3,
        "三节点响应式布局应返回 3 个 assignment"
    );
    assert!(
        approx_rect(
            assignments[0].bounds,
            CanvasRect::new(16.0, 16.0, 388.0, 242.5)
        ),
        "id=1 应位于窄屏单列第一行，实际 {:?}",
        assignments[0].bounds
    );
    assert!(
        approx_rect(
            assignments[1].bounds,
            CanvasRect::new(16.0, 274.5, 388.0, 242.5)
        ),
        "id=2 应位于窄屏单列第二行，实际 {:?}",
        assignments[1].bounds
    );
    assert!(
        (assignments[1].bounds.origin.x - 218.0).abs() > 1e-4,
        "id=2 不应出现在两列布局的第二列 x=218，否则窄屏未退回一列"
    );
}

/// 响应式编排：中等宽度可放两列但不够三列。
///
/// 数据构造（含关键数值推导）：
///   area = origin(0,0) size(760x700)
///   gap = 16, min_column_width = 320
///   candidate 2 宽度 = (760 - 16*(2+1)) / 2 = 356 >= 320 → 可用
///   candidate 3 宽度 = (760 - 16*(3+1)) / 3 = 232 < 320 → 不可用
///   columns = 2
///   tile_height = 356 / 1.6 = 222.5
///
/// 执行过程：
///   1. id=1 → row=0,col=0, origin=(16,16)
///   2. id=2 → row=0,col=1, origin=(388,16)
///   3. id=3 → row=1,col=0, origin=(16,254.5)
///
/// 预期结果：
///   - 正断言：id=2 位于第一行第二列
///   - 正断言：id=3 换到第二行第一列
///   - 负断言：id=3 不应位于第一行第三列
#[test]
fn test_responsive_grid_medium_area_uses_two_columns() {
    let assignments = compute_responsive_grid_layout(
        &[1, 2, 3],
        CanvasRect::new(0.0, 0.0, 760.0, 700.0),
        ResponsiveGridConfig {
            max_columns: 3,
            min_column_width: 320.0,
            aspect_ratio: 16.0 / 10.0,
            gap_canvas: 16.0,
            safe_area: LayoutInsets::ZERO,
        },
    );

    assert_eq!(
        assignments.len(),
        3,
        "三节点响应式布局应返回 3 个 assignment"
    );
    assert!(
        approx_rect(
            assignments[1].bounds,
            CanvasRect::new(388.0, 16.0, 356.0, 222.5)
        ),
        "id=2 应位于中宽两列布局第一行第二列，实际 {:?}",
        assignments[1].bounds
    );
    assert!(
        approx_rect(
            assignments[2].bounds,
            CanvasRect::new(16.0, 254.5, 356.0, 222.5)
        ),
        "id=3 应换到第二行第一列，实际 {:?}",
        assignments[2].bounds
    );
    assert!(
        assignments[2].bounds.origin.y > assignments[1].bounds.origin.y,
        "id=3 不应位于第一行第三列，否则中宽区域错误使用了三列"
    );
}

/// 响应式编排：宽区域使用三列并保留安全区。
///
/// 数据构造（含关键数值推导）：
///   area = origin(0,0) size(1280x800)
///   safe_area = left(96)+top(86)+right(32)+bottom(84)
///   usable = origin(96,86) size(1152x630)
///   gap = 16, min_column_width = 320
///   candidate 3 宽度 = (1152 - 16*(3+1)) / 3 = 362.6667 >= 320 → columns=3
///   tile_height = 362.6667 / 1.6 = 226.6667
///   id=1 origin=(96+16,86+16)=(112,102)
///   id=3 origin.x = 112 + (362.6667+16)*2 = 869.3334
///
/// 执行过程：
///   1. 先应用安全区得到 usable rect
///   2. 在 usable rect 内计算三列
///   3. 三个节点保持 row-major 位于第一行
///
/// 预期结果：
///   - 正断言：id=1 位于安全区之后的第一列
///   - 正断言：id=3 位于第一行第三列
///   - 负断言：id=1 不应从 x=16/y=16 开始，说明不能忽略安全区
#[test]
fn test_responsive_grid_wide_area_uses_three_columns_and_safe_area() {
    let assignments = compute_responsive_grid_layout(
        &[1, 2, 3],
        CanvasRect::new(0.0, 0.0, 1280.0, 800.0),
        ResponsiveGridConfig {
            max_columns: 3,
            min_column_width: 320.0,
            aspect_ratio: 16.0 / 10.0,
            gap_canvas: 16.0,
            safe_area: LayoutInsets::new(96.0, 86.0, 32.0, 84.0),
        },
    );

    assert_eq!(assignments.len(), 3, "三节点宽屏布局应返回 3 个 assignment");
    assert!(
        approx_rect(
            assignments[0].bounds,
            CanvasRect::new(112.0, 102.0, 362.66666, 226.66666)
        ),
        "id=1 应位于安全区后的第一列，实际 {:?}",
        assignments[0].bounds
    );
    assert!(
        approx_rect(
            assignments[2].bounds,
            CanvasRect::new(869.3333, 102.0, 362.66666, 226.66666)
        ),
        "id=3 应位于宽屏三列布局第一行第三列，实际 {:?}",
        assignments[2].bounds
    );
    assert!(
        !approx_rect(
            assignments[0].bounds,
            CanvasRect::new(16.0, 16.0, 394.66666, 246.66666)
        ),
        "id=1 不应忽略安全区从 (16,16) 开始"
    );
}
