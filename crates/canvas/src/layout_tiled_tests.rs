use super::*;
use crate::geometry::Vec2;

fn approx_vec(a: Vec2, b: Vec2) -> bool {
    (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4
}

fn approx_rect(a: CanvasRect, b: CanvasRect) -> bool {
    approx_vec(a.origin, b.origin) && approx_vec(a.size, b.size)
}

/// 默认卡片比例：在原 16:10 基础上增加 40% 后，再把当前卡片高度减少 10%。
///
/// 数据构造（含关键数值推导）：
///   base_aspect_ratio = 16 / 10 = 1.6
///   previous_height_multiplier = 1 + 40% = 1.4
///   default_height_multiplier = 1.4 * (1 - 10%) = 1.26
///   default_aspect_ratio = 1.6 / 1.26 = 1.2698413
///   area = origin(0,0) size(960x600), node_count=1, gap=8
///   tile_width = (960 - 8*(1+1)) / 1 = 944
///   old_height = 944 / 1.6 = 590
///   previous_height = 590 * 1.4 = 826
///   new_height = 944 / 1.2698413 = 743.4 = 826 * 0.9
///
/// 执行过程：
///   1. 使用 DEFAULT_CARD_ASPECT_RATIO 计算单节点布局
///   2. 宽度仍由 area 和 gap 决定，应保持 944
///   3. 高度按新比例计算，应比上一版默认高度减少 10%
///
/// 预期结果：
///   - 正断言：默认比例等于 16:10 / 1.26
///   - 正断言：单节点默认高度为 743.4
///   - 负断言：单节点默认高度不应仍是上一版高度 826
#[test]
fn test_default_card_aspect_ratio_reduces_current_height_by_ten_percent() {
    let assignments = compute_tiled_grid_layout(
        &[7],
        CanvasRect::new(0.0, 0.0, 960.0, 600.0),
        TiledGridConfig {
            max_columns: 3,
            aspect_ratio: DEFAULT_CARD_ASPECT_RATIO,
            gap_canvas: 8.0,
        },
    );

    let expected_height_multiplier = 1.4 * 0.9;
    assert!(
        (DEFAULT_CARD_ASPECT_RATIO - (BASE_CARD_ASPECT_RATIO / expected_height_multiplier)).abs()
            < 1e-6,
        "默认卡片比例应为 16:10 / 1.26，确保当前高度减少 10%"
    );
    assert_eq!(assignments.len(), 1, "单节点布局应返回 1 个 assignment");
    assert!(
        approx_rect(
            assignments[0].bounds,
            CanvasRect::new(8.0, 8.0, 944.0, 743.4)
        ),
        "默认卡片高度应从 826 减少 10% 到 743.4，实际 {:?}",
        assignments[0].bounds
    );
    assert!(
        !approx_rect(
            assignments[0].bounds,
            CanvasRect::new(8.0, 8.0, 944.0, 826.0)
        ),
        "默认卡片高度不应仍是上一版高度 826"
    );
}

/// 空节点列表三列平铺：没有参与者时不生成任何 assignment。
///
/// 数据构造（含关键数值推导）：
///   ordered_ids=[]
///   area = origin(0,0) size(960x600)
///   max_columns=3, aspect_ratio=16/10, gap_canvas=8
///   节点数为 0，因此 columns 不计算，gap/比例不应产生任何 bounds
///
/// 执行过程：
///   1. compute_tiled_grid_layout([], area, config)
///   2. 入口看到 ordered_ids 为空 → 直接返回空 Vec
///
/// 预期结果：
///   - 正断言：assignments.len() == 0
///   - 负断言：结果不应包含任何 node_id=1 的 assignment
#[test]
fn test_tiled_grid_empty_ids_returns_no_assignments() {
    let assignments = compute_tiled_grid_layout(
        &[],
        CanvasRect::new(0.0, 0.0, 960.0, 600.0),
        TiledGridConfig {
            max_columns: 3,
            aspect_ratio: 16.0 / 10.0,
            gap_canvas: 8.0,
        },
    );

    assert_eq!(assignments.len(), 0, "空节点列表不应生成任何 assignment");
    assert!(
        !assignments.iter().any(|assignment| assignment.node_id == 1),
        "空节点列表不应包含 node_id=1 的 assignment"
    );
}

/// 单节点三列平铺：唯一窗口使用整行宽度并按 16:10 计算高度。
///
/// 数据构造（含关键数值推导）：
///   area = origin(0,0) size(960x600)
///   node_count=1 → columns=min(1,3)=1
///   gap_canvas=8
///   tile_width = (960 - 8*(1+1)) / 1 = 944
///   tile_height = 944 / 1.6 = 590
///   origin = area.origin + (gap, gap) = (8,8)
///
/// 执行过程：
///   1. ordered_ids=[7] → row=0, col=0
///   2. 按公式计算 bounds=(8,8,944,590)
///   3. 生成 node_id=7 的 LayoutAssignment
///
/// 预期结果：
///   - 正断言：恰好生成 1 个 assignment
///   - 正断言：node_id=7 的 bounds 等于 (8,8,944,590)
///   - 负断言：结果不是无 gap 的整块 area
#[test]
fn test_tiled_grid_single_node_uses_full_row_width_with_aspect_ratio() {
    let assignments = compute_tiled_grid_layout(
        &[7],
        CanvasRect::new(0.0, 0.0, 960.0, 600.0),
        TiledGridConfig {
            max_columns: 3,
            aspect_ratio: 16.0 / 10.0,
            gap_canvas: 8.0,
        },
    );

    assert_eq!(assignments.len(), 1, "单节点布局应恰好返回 1 个 assignment");
    assert_eq!(
        assignments[0].node_id, 7,
        "唯一 assignment 应属于 node_id=7"
    );
    assert!(
        approx_rect(
            assignments[0].bounds,
            CanvasRect::new(8.0, 8.0, 944.0, 590.0)
        ),
        "单节点 bounds 应为 (8,8,944,590)，实际 {:?}",
        assignments[0].bounds
    );
    assert!(
        !approx_rect(
            assignments[0].bounds,
            CanvasRect::new(0.0, 0.0, 960.0, 600.0)
        ),
        "gap_canvas=8 时不应返回原始无 gap 的 area"
    );
}

/// 双节点三列平铺：节点少于三时列数等于节点数，两个窗口同一行等宽。
///
/// 数据构造（含关键数值推导）：
///   area = origin(0,0) size(960x600)
///   node_count=2 → columns=min(2,3)=2
///   gap_canvas=8
///   tile_width = (960 - 8*(2+1)) / 2 = 468
///   tile_height = 468 / 1.6 = 292.5
///   id=1 origin=(8,8)
///   id=2 origin=(8 + (468+8)*1, 8) = (484,8)
///
/// 执行过程：
///   1. id=1 → row=0, col=0
///   2. id=2 → row=0, col=1
///   3. 两个节点使用相同 size=(468,292.5)
///
/// 预期结果：
///   - 正断言：id=1 在第一列
///   - 正断言：id=2 在第二列
///   - 负断言：id=2 不应换到第二行
#[test]
fn test_tiled_grid_two_nodes_share_one_row_equally() {
    let assignments = compute_tiled_grid_layout(
        &[1, 2],
        CanvasRect::new(0.0, 0.0, 960.0, 600.0),
        TiledGridConfig {
            max_columns: 3,
            aspect_ratio: 16.0 / 10.0,
            gap_canvas: 8.0,
        },
    );

    assert_eq!(assignments.len(), 2, "双节点布局应返回 2 个 assignment");
    assert_eq!(
        assignments[0].node_id, 1,
        "第一个节点应保持顺序分配到第一列"
    );
    assert_eq!(
        assignments[1].node_id, 2,
        "第二个节点应保持顺序分配到第二列"
    );
    assert!(
        approx_rect(
            assignments[0].bounds,
            CanvasRect::new(8.0, 8.0, 468.0, 292.5)
        ),
        "id=1 应占据第一列 (8,8,468,292.5)，实际 {:?}",
        assignments[0].bounds
    );
    assert!(
        approx_rect(
            assignments[1].bounds,
            CanvasRect::new(484.0, 8.0, 468.0, 292.5)
        ),
        "id=2 应占据第二列 (484,8,468,292.5)，实际 {:?}",
        assignments[1].bounds
    );
    assert!(
        assignments[1].bounds.origin.y != 308.5,
        "id=2 不应换到第二行，否则双节点没有共享同一行"
    );
}

/// 三节点三列平铺：达到最大列数时三个窗口同一行三等分。
///
/// 数据构造（含关键数值推导）：
///   area = origin(0,0) size(960x600)
///   node_count=3 → columns=min(3,3)=3
///   gap_canvas=8
///   tile_width = (960 - 8*(3+1)) / 3 = 309.33334
///   tile_height = 309.33334 / 1.6 = 193.33334
///   col origins:
///     col0 x = 8
///     col1 x = 8 + (309.33334+8)*1 = 325.33334
///     col2 x = 8 + (309.33334+8)*2 = 642.6667
///
/// 执行过程：
///   1. id=1/id=2/id=3 均在 row=0
///   2. col 按 0/1/2 递增
///   3. 三个节点使用相同 size
///
/// 预期结果：
///   - 正断言：三个节点分别在第一行三列
///   - 正断言：第三个节点右边界为 952，保留右侧 8 gap
///   - 负断言：第三个节点不应换到第二行
#[test]
fn test_tiled_grid_three_nodes_use_three_columns() {
    let assignments = compute_tiled_grid_layout(
        &[1, 2, 3],
        CanvasRect::new(0.0, 0.0, 960.0, 600.0),
        TiledGridConfig {
            max_columns: 3,
            aspect_ratio: 16.0 / 10.0,
            gap_canvas: 8.0,
        },
    );

    assert_eq!(assignments.len(), 3, "三节点布局应返回 3 个 assignment");
    assert!(
        approx_rect(
            assignments[0].bounds,
            CanvasRect::new(8.0, 8.0, 309.33334, 193.33334)
        ),
        "id=1 应位于第一行第一列，实际 {:?}",
        assignments[0].bounds
    );
    assert!(
        approx_rect(
            assignments[1].bounds,
            CanvasRect::new(325.33334, 8.0, 309.33334, 193.33334)
        ),
        "id=2 应位于第一行第二列，实际 {:?}",
        assignments[1].bounds
    );
    assert!(
        approx_rect(
            assignments[2].bounds,
            CanvasRect::new(642.6667, 8.0, 309.33334, 193.33334)
        ),
        "id=3 应位于第一行第三列，实际 {:?}",
        assignments[2].bounds
    );
    assert!(
        (assignments[2].bounds.max().x - 952.0).abs() < 1e-4,
        "id=3 右边界应为 952，给右侧保留 8 gap，实际 {}",
        assignments[2].bounds.max().x
    );
    assert!(
        assignments[2].bounds.origin.y != 209.33334,
        "id=3 不应换到第二行，否则三列上限未正确生效"
    );
}

/// 四节点三列平铺：第四个窗口进入第二行第一列，尾行保持三分之一列宽。
///
/// 数据构造（含关键数值推导）：
///   area = origin(10,20) size(960x600)
///   node_count=4 → columns=min(4,3)=3
///   gap_canvas=8
///   tile_width = (960 - 8*(3+1)) / 3 = 309.33334
///   tile_height = 309.33334 / 1.6 = 193.33334
///   row0 y = 20 + 8 = 28
///   row1 y = 28 + 193.33334 + 8 = 229.33334
///   id=4 row=1, col=0 → origin=(18,229.33334)
///
/// 执行过程：
///   1. id=1/id=2/id=3 分配到第一行三列
///   2. id=4 分配到第二行第一列
///   3. 尾行不铺满，因此 id=4 width 仍为 309.33334
///
/// 预期结果：
///   - 正断言：id=4 在第二行第一列
///   - 正断言：id=4 width 与 id=1 相同
///   - 负断言：id=4 不应独占整行宽度
#[test]
fn test_tiled_grid_fourth_node_wraps_to_next_row_without_filling_tail_row() {
    let assignments = compute_tiled_grid_layout(
        &[1, 2, 3, 4],
        CanvasRect::new(10.0, 20.0, 960.0, 600.0),
        TiledGridConfig {
            max_columns: 3,
            aspect_ratio: 16.0 / 10.0,
            gap_canvas: 8.0,
        },
    );

    assert_eq!(assignments.len(), 4, "四节点布局应返回 4 个 assignment");
    assert!(
        approx_rect(
            assignments[0].bounds,
            CanvasRect::new(18.0, 28.0, 309.33334, 193.33334)
        ),
        "id=1 应位于第一行第一列，实际 {:?}",
        assignments[0].bounds
    );
    assert!(
        approx_rect(
            assignments[3].bounds,
            CanvasRect::new(18.0, 229.33334, 309.33334, 193.33334)
        ),
        "id=4 应位于第二行第一列，实际 {:?}",
        assignments[3].bounds
    );
    assert!(
        (assignments[3].bounds.size.x - assignments[0].bounds.size.x).abs() < 1e-4,
        "id=4 尾行宽度应与第一行列宽相同，实际 {} vs {}",
        assignments[3].bounds.size.x,
        assignments[0].bounds.size.x
    );
    assert!(
        (assignments[3].bounds.size.x - 944.0).abs() > 1e-4,
        "id=4 不应独占尾行整行宽度 944，否则尾行被错误铺满"
    );
}
