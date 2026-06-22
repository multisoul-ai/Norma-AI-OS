use super::*;
use crate::geometry::Vec2;

fn approx_vec(a: Vec2, b: Vec2) -> bool {
    (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4
}

fn approx_rect(a: CanvasRect, b: CanvasRect) -> bool {
    approx_vec(a.origin, b.origin) && approx_vec(a.size, b.size)
}

/// Master/detail 布局：四张卡时 master 占左侧 2/3，另外三张在右侧 1/3 纵向自适应。
///
/// 数据构造（含关键数值的推导过程）：
///   ordered_ids = [1,2,3,4]，master_id = 1
///   area = origin(0,0), size(1000x600)
///   safe_area = 0 → usable = 1000x600
///   gap = 16
///   content_width = 1000 - 16*3 = 952
///   master_width = 952 * 2 / 3 = 634.6667
///   master = x=16,y=16,w=634.6667,h=600-16*2=568
///   detail area = x=650.6667,w=349.3333；三张 detail 单列：
///     tile_width = 349.3333 - 16*2 = 317.3333
///     tile_height = (600 - 16*4) / 3 = 178.6667
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 识别 master_id=1 存在于 ordered_ids
///   2. 在安全区内按 2/3 master + 1/3 detail 切分
///   3. master 放入左侧 2/3；[2,3,4] 交给响应式网格放入右侧 1/3
///
/// 预期结果：
///   - 正断言：id=1 位于左侧 2/3 master 区
///   - 正断言：id=2/id=3/id=4 位于右侧 1/3 同一列并纵向排列
///   - 负断言：id=2 不应出现在左侧 master 的 x=16
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
            CanvasRect::new(16.0, 16.0, 634.6667, 568.0)
        ),
        "id=1 应占据左侧 2/3 master 区，实际 {:?}",
        assignments[0].bounds
    );
    assert!(
        approx_rect(
            assignments[1].bounds,
            CanvasRect::new(666.6667, 16.0, 317.3333, 178.66667)
        ),
        "id=2 应位于右侧 1/3 第一行，实际 {:?}",
        assignments[1].bounds
    );
    assert!(
        approx_rect(
            assignments[2].bounds,
            CanvasRect::new(666.6667, 210.66667, 317.3333, 178.66667)
        ),
        "id=3 应位于右侧 1/3 第二行，实际 {:?}",
        assignments[2].bounds
    );
    assert!(
        approx_rect(
            assignments[3].bounds,
            CanvasRect::new(666.6667, 405.33334, 317.3333, 178.66667)
        ),
        "id=4 应位于右侧 1/3 第三行，实际 {:?}",
        assignments[3].bounds
    );
    assert_ne!(
        assignments[1].bounds.origin.x, 16.0,
        "detail id=2 不应占用左侧 master 的 x=16"
    );
}

/// Master/detail 布局：master 是中间节点时，右侧 detail 保持原文档顺序并排除 master。
///
/// 数据构造（含关键数值的推导过程）：
///   ordered_ids = [1,2,3,4]，master_id = 3
///   detail_ids 应为 [1,2,4]，不能排序为 [1,2,3] 或把 master 留在右侧
///   area/gap 与上个测试相同，因此左侧 2/3 master bounds 仍为 (16,16,634.6667,568)
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
