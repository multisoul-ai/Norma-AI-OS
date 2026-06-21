use super::*;
use crate::geometry::Vec2;

fn approx_vec(a: Vec2, b: Vec2) -> bool {
    (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4
}

fn approx_rect(a: CanvasRect, b: CanvasRect) -> bool {
    approx_vec(a.origin, b.origin) && approx_vec(a.size, b.size)
}

/// Master/detail 布局：四张卡时 master 占左半屏，另外三张在右半屏纵向自适应。
///
/// 数据构造（含关键数值的推导过程）：
///   ordered_ids = [1,2,3,4]，master_id = 1
///   area = origin(0,0), size(1000x600)
///   safe_area = 0 → usable = 1000x600
///   gap = 16
///   panel_width = (1000 - 16*3) / 2 = 476
///   master = x=16,y=16,w=476,h=600-16*2=568
///   detail area = x=492,w=508；三张 detail 单列：
///     tile_width = 508 - 16*2 = 476
///     tile_height = (600 - 16*4) / 3 = 178.6667
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 识别 master_id=1 存在于 ordered_ids
///   2. 在安全区内按左右两栏切分
///   3. master 放入左栏；[2,3,4] 交给响应式网格放入右栏
///
/// 预期结果：
///   - 正断言：id=1 位于左半屏 master 区
///   - 正断言：id=2/id=3/id=4 位于右半屏同一列并纵向排列
///   - 负断言：id=2 不应出现在左半屏 master 的 x=16
#[test]
fn test_master_detail_layout_places_master_left_and_details_right_column() {
    let assignments = compute_master_detail_layout(
        &[1, 2, 3, 4],
        1,
        CanvasRect::new(0.0, 0.0, 1000.0, 600.0),
        ResponsiveGridConfig {
            max_columns: 3,
            min_column_width: 320.0,
            aspect_ratio: DEFAULT_CARD_ASPECT_RATIO,
            gap_canvas: 16.0,
            safe_area: LayoutInsets::ZERO,
        },
    );

    assert_eq!(
        assignments.len(),
        4,
        "四张卡 master/detail 布局应返回 4 个 assignment"
    );
    assert!(
        approx_rect(
            assignments[0].bounds,
            CanvasRect::new(16.0, 16.0, 476.0, 568.0)
        ),
        "id=1 应占据左半屏 master 区，实际 {:?}",
        assignments[0].bounds
    );
    assert!(
        approx_rect(
            assignments[1].bounds,
            CanvasRect::new(508.0, 16.0, 476.0, 178.66667)
        ),
        "id=2 应位于右半屏第一行，实际 {:?}",
        assignments[1].bounds
    );
    assert!(
        approx_rect(
            assignments[2].bounds,
            CanvasRect::new(508.0, 210.66667, 476.0, 178.66667)
        ),
        "id=3 应位于右半屏第二行，实际 {:?}",
        assignments[2].bounds
    );
    assert!(
        approx_rect(
            assignments[3].bounds,
            CanvasRect::new(508.0, 405.33334, 476.0, 178.66667)
        ),
        "id=4 应位于右半屏第三行，实际 {:?}",
        assignments[3].bounds
    );
    assert_ne!(
        assignments[1].bounds.origin.x, 16.0,
        "detail id=2 不应占用左半屏 master 的 x=16"
    );
}

/// Master/detail 布局：master 是中间节点时，右侧 detail 保持原文档顺序并排除 master。
///
/// 数据构造（含关键数值的推导过程）：
///   ordered_ids = [1,2,3,4]，master_id = 3
///   detail_ids 应为 [1,2,4]，不能排序为 [1,2,3] 或把 master 留在右侧
///   area/gap 与上个测试相同，因此左栏 master bounds 仍为 (16,16,476,568)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从 ordered_ids 过滤 master_id=3
///   2. 输出顺序为 master assignment 在前，detail assignments 之后按 [1,2,4]
///   3. 检查输出 id 顺序与 bounds
///
/// 预期结果：
///   - 正断言：第一个 assignment 是 master id=3
///   - 正断言：右侧 detail 顺序为 id=1、id=2、id=4
///   - 负断言：右侧不应包含 id=3
#[test]
fn test_master_detail_layout_keeps_detail_order_when_master_is_middle_node() {
    let assignments = compute_master_detail_layout(
        &[1, 2, 3, 4],
        3,
        CanvasRect::new(0.0, 0.0, 1000.0, 600.0),
        ResponsiveGridConfig {
            max_columns: 3,
            min_column_width: 320.0,
            aspect_ratio: DEFAULT_CARD_ASPECT_RATIO,
            gap_canvas: 16.0,
            safe_area: LayoutInsets::ZERO,
        },
    );

    assert_eq!(assignments.len(), 4, "四张卡应返回 4 个 assignment");
    assert_eq!(
        assignments[0].node_id, 3,
        "master_id=3 时第一个 assignment 应是 master"
    );
    assert_eq!(
        assignments[1].node_id, 1,
        "第一个 detail 应保持原文档顺序中的 id=1"
    );
    assert_eq!(
        assignments[2].node_id, 2,
        "第二个 detail 应保持原文档顺序中的 id=2"
    );
    assert_eq!(
        assignments[3].node_id, 4,
        "第三个 detail 应跳过 master 后保留 id=4"
    );
    assert!(
        !assignments[1..]
            .iter()
            .any(|assignment| assignment.node_id == 3),
        "右侧 detail 不应包含 master id=3"
    );
}

/// Master/detail 布局：master_id 不存在时返回空布局，避免错误移动全部卡片。
///
/// 数据构造（含关键数值的推导过程）：
///   ordered_ids = [1,2,4]，master_id = 3
///   3 不在节点列表中，说明 hover/selection 目标已失效
///
/// 执行过程（逐步说明系统如何处理）：
///   1. compute_master_detail_layout 先检查 master_id 是否存在
///   2. 不存在 → 返回空 Vec
///
/// 预期结果：
///   - 正断言：结果为空，调用方可回退普通 reflow 或忽略
///   - 负断言：结果不应包含 id=1，避免失效 master 触发错误布局
#[test]
fn test_master_detail_layout_missing_master_returns_empty_assignments() {
    let assignments = compute_master_detail_layout(
        &[1, 2, 4],
        3,
        CanvasRect::new(0.0, 0.0, 1000.0, 600.0),
        ResponsiveGridConfig {
            max_columns: 3,
            min_column_width: 320.0,
            aspect_ratio: DEFAULT_CARD_ASPECT_RATIO,
            gap_canvas: 16.0,
            safe_area: LayoutInsets::ZERO,
        },
    );

    assert!(
        assignments.is_empty(),
        "master_id=3 不存在时应返回空布局，避免错误移动节点"
    );
    assert!(
        !assignments.iter().any(|assignment| assignment.node_id == 1),
        "失效 master 布局不应仍生成 id=1 的 assignment"
    );
}
