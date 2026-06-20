//! 即时 BSP 布局：不保存树，每次根据节点顺序重新计算平铺 bounds。

use crate::geometry::CanvasRect;
use crate::node::NodeId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InstantBspConfig {
    pub gap_canvas: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutAssignment {
    pub node_id: NodeId,
    pub bounds: CanvasRect,
}

pub fn compute_instant_bsp_layout(
    ordered_ids: &[NodeId],
    area: CanvasRect,
    config: InstantBspConfig,
) -> Vec<LayoutAssignment> {
    let mut assignments = Vec::with_capacity(ordered_ids.len());
    assign_recursive(ordered_ids, area, config.gap_canvas, &mut assignments);
    assignments
}

fn assign_recursive(
    ordered_ids: &[NodeId],
    area: CanvasRect,
    gap_canvas: f32,
    assignments: &mut Vec<LayoutAssignment>,
) {
    match ordered_ids.len() {
        0 => {}
        1 => assignments.push(LayoutAssignment {
            node_id: ordered_ids[0],
            bounds: area.inset(gap_canvas),
        }),
        len => {
            let first_count = len.div_ceil(2);
            let second_count = len - first_count;
            let split_ratio = first_count as f32 / len as f32;
            let (first_area, second_area) = split_area(area, split_ratio);
            assign_recursive(
                &ordered_ids[..first_count],
                first_area,
                gap_canvas,
                assignments,
            );
            assign_recursive(
                &ordered_ids[first_count..first_count + second_count],
                second_area,
                gap_canvas,
                assignments,
            );
        }
    }
}

fn split_area(area: CanvasRect, split_ratio: f32) -> (CanvasRect, CanvasRect) {
    if area.size.x > area.size.y {
        let first_w = area.size.x * split_ratio;
        let second_w = area.size.x - first_w;
        (
            CanvasRect::new(area.origin.x, area.origin.y, first_w, area.size.y),
            CanvasRect::new(
                area.origin.x + first_w,
                area.origin.y,
                second_w,
                area.size.y,
            ),
        )
    } else {
        let first_h = area.size.y * split_ratio;
        let second_h = area.size.y - first_h;
        (
            CanvasRect::new(area.origin.x, area.origin.y, area.size.x, first_h),
            CanvasRect::new(
                area.origin.x,
                area.origin.y + first_h,
                area.size.x,
                second_h,
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Vec2;

    fn approx_vec(a: Vec2, b: Vec2) -> bool {
        (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4
    }

    fn approx_rect(a: CanvasRect, b: CanvasRect) -> bool {
        approx_vec(a.origin, b.origin) && approx_vec(a.size, b.size)
    }

    /// 空节点列表即时 BSP：没有参与者时不生成任何 assignment。
    ///
    /// 数据构造（含关键数值推导）：
    ///   ordered_ids=[]
    ///   area = origin(0,0) size(100x60)
    ///   gap_canvas=4，但无叶子节点，因此 gap 不应产生任何 bounds
    ///
    /// 执行过程：
    ///   1. compute_instant_bsp_layout([], area, gap=4)
    ///   2. 递归入口看到 len=0 → 直接返回
    ///
    /// 预期结果：
    ///   - 正断言：assignments.len() == 0
    ///   - 负断言：结果不应包含任何 node_id=1 的 assignment
    #[test]
    fn test_instant_bsp_empty_ids_returns_no_assignments() {
        let assignments = compute_instant_bsp_layout(
            &[],
            CanvasRect::new(0.0, 0.0, 100.0, 60.0),
            InstantBspConfig { gap_canvas: 4.0 },
        );

        assert_eq!(assignments.len(), 0, "空节点列表不应生成任何 assignment");
        assert!(
            !assignments.iter().any(|assignment| assignment.node_id == 1),
            "空节点列表不应包含 node_id=1 的 assignment"
        );
    }

    /// 单节点即时 BSP：唯一节点填满目标区域并应用 gap inset。
    ///
    /// 数据构造（含关键数值推导）：
    ///   area = origin(0,0) size(100x60)
    ///   gap_canvas = 4
    ///   inset 后 origin = (0+4, 0+4) = (4,4)
    ///   inset 后 size = (100-4*2, 60-4*2) = (92,52)
    ///
    /// 执行过程：
    ///   1. ordered_ids=[7] → 只有一个叶子区域
    ///   2. 对叶子 area 应用 gap inset
    ///   3. 生成 node_id=7 的 LayoutAssignment
    ///
    /// 预期结果：
    ///   - 正断言：恰好生成 1 个 assignment
    ///   - 正断言：node_id=7 的 bounds 等于 inset 后矩形
    ///   - 负断言：结果不是原始无 gap area
    #[test]
    fn test_instant_bsp_single_node_fills_area_minus_gap() {
        let assignments = compute_instant_bsp_layout(
            &[7],
            CanvasRect::new(0.0, 0.0, 100.0, 60.0),
            InstantBspConfig { gap_canvas: 4.0 },
        );

        assert_eq!(assignments.len(), 1, "单节点布局应恰好返回 1 个 assignment");
        assert_eq!(
            assignments[0].node_id, 7,
            "唯一 assignment 应属于 node_id=7"
        );
        assert!(
            approx_rect(assignments[0].bounds, CanvasRect::new(4.0, 4.0, 92.0, 52.0)),
            "单节点 bounds 应为 gap inset 后的 (4,4,92,52)，实际 {:?}",
            assignments[0].bounds
        );
        assert!(
            !approx_rect(
                assignments[0].bounds,
                CanvasRect::new(0.0, 0.0, 100.0, 60.0)
            ),
            "gap_canvas=4 时不应返回原始无 inset 的 area"
        );
    }

    /// 双节点即时 BSP：沿更长边横向切分，保持节点顺序。
    ///
    /// 数据构造（含关键数值推导）：
    ///   area = origin(0,0) size(100x60)，宽 100 > 高 60
    ///   ordered_ids=[1,2]，left_count=(2+1)/2=1，right_count=1
    ///   split_x = 100 * 1/2 = 50
    ///   gap_canvas=0，所以两个叶子不 inset
    ///
    /// 执行过程：
    ///   1. 根区域沿 x 轴切为 left(0,0,50,60)、right(50,0,50,60)
    ///   2. id=1 分配到左半区
    ///   3. id=2 分配到右半区
    ///
    /// 预期结果：
    ///   - 正断言：id=1 在左半区
    ///   - 正断言：id=2 在右半区
    ///   - 负断言：id=1 不应被放到右半区
    #[test]
    fn test_instant_bsp_two_nodes_split_longer_axis() {
        let assignments = compute_instant_bsp_layout(
            &[1, 2],
            CanvasRect::new(0.0, 0.0, 100.0, 60.0),
            InstantBspConfig { gap_canvas: 0.0 },
        );

        assert_eq!(assignments.len(), 2, "双节点布局应返回 2 个 assignment");
        assert_eq!(
            assignments[0].node_id, 1,
            "第一个节点应保持顺序分配到左半区"
        );
        assert_eq!(
            assignments[1].node_id, 2,
            "第二个节点应保持顺序分配到右半区"
        );
        assert!(
            approx_rect(assignments[0].bounds, CanvasRect::new(0.0, 0.0, 50.0, 60.0)),
            "id=1 应占据左半区 (0,0,50,60)，实际 {:?}",
            assignments[0].bounds
        );
        assert!(
            approx_rect(
                assignments[1].bounds,
                CanvasRect::new(50.0, 0.0, 50.0, 60.0)
            ),
            "id=2 应占据右半区 (50,0,50,60)，实际 {:?}",
            assignments[1].bounds
        );
        assert!(
            !approx_rect(
                assignments[0].bounds,
                CanvasRect::new(50.0, 0.0, 50.0, 60.0)
            ),
            "id=1 不应被分配到右半区，否则节点顺序被打乱"
        );
    }

    /// 三节点即时 BSP：根切分比例按节点数 2:1，而不是固定 50:50。
    ///
    /// 数据构造（含关键数值推导）：
    ///   area = origin(0,0) size(90x60)，宽 90 > 高 60
    ///   ordered_ids=[1,2,3]
    ///   left_count=(3+1)/2=2，right_count=1
    ///   根 split_x = 90 * 2/3 = 60，所以左侧容纳 2 个节点、右侧容纳 1 个节点
    ///   左侧子区域 size(60x60)，宽高相等时按垂直切分为上/下各 30 高
    ///
    /// 执行过程：
    ///   1. 根区域切成 left(0,0,60,60)、right(60,0,30,60)
    ///   2. id=1/id=2 在左侧递归切成上下两块
    ///   3. id=3 占据右侧整块
    ///
    /// 预期结果：
    ///   - 正断言：id=3 bounds.x=60 且 width=30，证明根比例是 2:1
    ///   - 正断言：id=1/id=2 分别在左侧上下两块
    ///   - 负断言：id=3 不应从 x=45 开始（那是错误的 50:50 根切分）
    #[test]
    fn test_instant_bsp_three_nodes_uses_count_weighted_root_split() {
        let assignments = compute_instant_bsp_layout(
            &[1, 2, 3],
            CanvasRect::new(0.0, 0.0, 90.0, 60.0),
            InstantBspConfig { gap_canvas: 0.0 },
        );

        assert_eq!(assignments.len(), 3, "三节点布局应返回 3 个 assignment");
        assert!(
            approx_rect(assignments[0].bounds, CanvasRect::new(0.0, 0.0, 60.0, 30.0)),
            "id=1 应在左侧上半区 (0,0,60,30)，实际 {:?}",
            assignments[0].bounds
        );
        assert!(
            approx_rect(
                assignments[1].bounds,
                CanvasRect::new(0.0, 30.0, 60.0, 30.0)
            ),
            "id=2 应在左侧下半区 (0,30,60,30)，实际 {:?}",
            assignments[1].bounds
        );
        assert!(
            approx_rect(
                assignments[2].bounds,
                CanvasRect::new(60.0, 0.0, 30.0, 60.0)
            ),
            "id=3 应占据右侧 1/3 区域 (60,0,30,60)，实际 {:?}",
            assignments[2].bounds
        );
        assert!(
            assignments[2].bounds.origin.x != 45.0,
            "三节点根切分不应从 x=45 开始，否则说明错误使用了 50:50 切分"
        );
    }

    /// 四节点即时 BSP：递归二分产生四个互不重叠的平铺矩形。
    ///
    /// 数据构造（含关键数值推导）：
    ///   area = origin(0,0) size(100x80)
    ///   ordered_ids=[1,2,3,4]
    ///   root: left_count=(4+1)/2=2，right_count=2 → split_x = 100*2/4 = 50
    ///   left 子区域 size(50x80)，高 80 > 宽 50 → 上下切分为 40/40
    ///   right 子区域同理上下切分为 40/40
    ///
    /// 执行过程：
    ///   1. 根区域左右二分
    ///   2. 左侧 id=1/id=2 上下二分
    ///   3. 右侧 id=3/id=4 上下二分
    ///
    /// 预期结果：
    ///   - 正断言：四个节点分别占据预期象限
    ///   - 负断言：相邻矩形之间不应发生面积重叠
    #[test]
    fn test_instant_bsp_four_nodes_are_non_overlapping_tiles() {
        let assignments = compute_instant_bsp_layout(
            &[1, 2, 3, 4],
            CanvasRect::new(0.0, 0.0, 100.0, 80.0),
            InstantBspConfig { gap_canvas: 0.0 },
        );

        assert_eq!(assignments.len(), 4, "四节点布局应返回 4 个 assignment");
        assert!(
            approx_rect(assignments[0].bounds, CanvasRect::new(0.0, 0.0, 50.0, 40.0)),
            "id=1 应位于左上 tile，实际 {:?}",
            assignments[0].bounds
        );
        assert!(
            approx_rect(
                assignments[1].bounds,
                CanvasRect::new(0.0, 40.0, 50.0, 40.0)
            ),
            "id=2 应位于左下 tile，实际 {:?}",
            assignments[1].bounds
        );
        assert!(
            approx_rect(
                assignments[2].bounds,
                CanvasRect::new(50.0, 0.0, 50.0, 40.0)
            ),
            "id=3 应位于右上 tile，实际 {:?}",
            assignments[2].bounds
        );
        assert!(
            approx_rect(
                assignments[3].bounds,
                CanvasRect::new(50.0, 40.0, 50.0, 40.0)
            ),
            "id=4 应位于右下 tile，实际 {:?}",
            assignments[3].bounds
        );
        assert!(
            !assignments[0].bounds.intersects(assignments[1].bounds),
            "id=1 左上 tile 与 id=2 左下 tile 共享边界但不应面积重叠"
        );
        assert!(
            !assignments[0].bounds.intersects(assignments[2].bounds),
            "id=1 左上 tile 与 id=3 右上 tile 共享边界但不应面积重叠"
        );
        assert!(
            !assignments[1].bounds.intersects(assignments[3].bounds),
            "id=2 左下 tile 与 id=4 右下 tile 共享边界但不应面积重叠"
        );
        assert!(
            !assignments[2].bounds.intersects(assignments[3].bounds),
            "id=3 右上 tile 与 id=4 右下 tile 共享边界但不应面积重叠"
        );
    }
}
