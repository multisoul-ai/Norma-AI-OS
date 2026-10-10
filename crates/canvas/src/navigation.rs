//! Keyboard-style spatial navigation between canvas nodes.

use crate::{CanvasNode, CanvasRect, NodeId, Vec2};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NavigationDirection {
    Up,
    Down,
    Left,
    Right,
}

pub fn navigate_card(
    nodes: &[CanvasNode],
    selected_id: Option<NodeId>,
    visible_area: CanvasRect,
    direction: NavigationDirection,
) -> Option<NodeId> {
    let anchor_id = selected_id
        .and_then(|id| nodes.iter().find(|node| node.id == id).map(|node| node.id))
        .or_else(|| nearest_node_to_point(nodes, rect_center(visible_area)))?;
    let anchor = nodes.iter().find(|node| node.id == anchor_id)?;
    let anchor_center = rect_center(anchor.bounds);

    nodes
        .iter()
        .filter(|node| node.id != anchor_id)
        .filter_map(|node| {
            let center = rect_center(node.bounds);
            let (primary, lateral) = directional_distances(anchor_center, center, direction)?;
            let score = lateral * 2.0 + primary;
            Some((node.id, score, primary))
        })
        .min_by(|a, b| {
            a.1.total_cmp(&b.1)
                .then_with(|| a.2.total_cmp(&b.2))
                .then_with(|| a.0.cmp(&b.0))
        })
        .map(|(id, _, _)| id)
}

fn nearest_node_to_point(nodes: &[CanvasNode], point: Vec2) -> Option<NodeId> {
    nodes
        .iter()
        .map(|node| {
            let center = rect_center(node.bounds);
            let dx = center.x - point.x;
            let dy = center.y - point.y;
            (node.id, dx * dx + dy * dy)
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
        .map(|(id, _)| id)
}

fn rect_center(rect: CanvasRect) -> Vec2 {
    Vec2::new(
        rect.origin.x + rect.size.x / 2.0,
        rect.origin.y + rect.size.y / 2.0,
    )
}

fn directional_distances(
    anchor: Vec2,
    candidate: Vec2,
    direction: NavigationDirection,
) -> Option<(f32, f32)> {
    let dx = candidate.x - anchor.x;
    let dy = candidate.y - anchor.y;
    match direction {
        NavigationDirection::Right if dx > 0.0 => Some((dx, dy.abs())),
        NavigationDirection::Left if dx < 0.0 => Some((-dx, dy.abs())),
        NavigationDirection::Down if dy > 0.0 => Some((dy, dx.abs())),
        NavigationDirection::Up if dy < 0.0 => Some((-dy, dx.abs())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CanvasNodeKind, RectNode};

    fn node(id: NodeId, x: f32, y: f32) -> CanvasNode {
        CanvasNode {
            card_instance_id: crate::new_card_instance_id(),
            id,
            kind: CanvasNodeKind::Rect(RectNode { color: 0x4080ff }),
            bounds: CanvasRect::new(x, y, 100.0, 80.0),
            z_index: id as i32,
            selected: false,
        }
    }

    /// 方向键空间导航：从中心卡片出发，四个方向都选到几何方向最近的卡片。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   center id=1 bounds=(100,100,100,80) center=(150,140)
    ///   right  id=2 center=(290,140)，dx=140, dy=0，Right score=0*2+140=140
    ///   left   id=3 center=(10,140)， dx=-140, dy=0，Left  score=140
    ///   up     id=4 center=(150,0)，  dx=0, dy=-140，Up    score=140
    ///   down   id=5 center=(150,280)，dx=0, dy=140，Down  score=140
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. selected_id=Some(1)，以 center(150,140) 作为起点
    ///   2. 分别执行 Right/Left/Up/Down
    ///   3. 每个方向只在对应半平面内挑选最小 score 候选
    ///
    /// 预期结果：
    ///   - 正断言：Right/Left/Up/Down 分别返回 2/3/4/5
    ///   - 负断言：Right 不应返回左侧 id=3，Up 不应返回下方 id=5
    #[test]
    fn test_navigate_card_selects_spatial_neighbor_in_each_direction() {
        let nodes = vec![
            node(1, 100.0, 100.0),
            node(2, 240.0, 100.0),
            node(3, -40.0, 100.0),
            node(4, 100.0, -40.0),
            node(5, 100.0, 240.0),
        ];
        let visible = CanvasRect::new(0.0, 0.0, 400.0, 320.0);

        let right = navigate_card(&nodes, Some(1), visible, NavigationDirection::Right);
        let left = navigate_card(&nodes, Some(1), visible, NavigationDirection::Left);
        let up = navigate_card(&nodes, Some(1), visible, NavigationDirection::Up);
        let down = navigate_card(&nodes, Some(1), visible, NavigationDirection::Down);

        assert_eq!(right, Some(2), "Right 应从 id=1 选到右侧最近卡片 id=2");
        assert_eq!(left, Some(3), "Left 应从 id=1 选到左侧最近卡片 id=3");
        assert_eq!(up, Some(4), "Up 应从 id=1 选到上方最近卡片 id=4");
        assert_eq!(down, Some(5), "Down 应从 id=1 选到下方最近卡片 id=5");
        assert_ne!(right, Some(3), "Right 不应反向选到左侧卡片 id=3");
        assert_ne!(up, Some(5), "Up 不应反向选到下方卡片 id=5");
    }

    /// 无当前选择时，导航以可见画布中心附近卡片作为起点继续寻找方向候选。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   visible area=(0,0,500,300)，visible center=(250,150)
    ///   centerish id=10 center=(250,150)，距离视口中心 0，作为导航起点
    ///   right    id=11 center=(390,150)，Right primary=140, lateral=0, score=140
    ///   far left id=12 center=(50,150)，位于起点左侧，不是 Right 候选
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. selected_id=None → 找到离 visible center 最近的 id=10 作为起点
    ///   2. 执行 Right 导航
    ///   3. 在 id=10 右侧候选中选择 score 最小的 id=11
    ///
    /// 预期结果：
    ///   - 正断言：返回 Some(11)
    ///   - 负断言：不返回起点 id=10，也不返回左侧 id=12
    #[test]
    fn test_navigate_card_without_selection_uses_visible_center_anchor() {
        let nodes = vec![
            node(10, 200.0, 110.0),
            node(11, 340.0, 110.0),
            node(12, 0.0, 110.0),
        ];
        let visible = CanvasRect::new(0.0, 0.0, 500.0, 300.0);

        let result = navigate_card(&nodes, None, visible, NavigationDirection::Right);

        assert_eq!(
            result,
            Some(11),
            "无选择时应以视口中心附近 id=10 为起点，再向右选择 id=11"
        );
        assert_ne!(result, Some(10), "方向导航不应返回起点自身 id=10");
        assert_ne!(result, Some(12), "Right 不应选择位于起点左侧的 id=12");
    }

    /// 当前选择已失效时，应回退到可见画布中心附近卡片作为起点。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   selected_id=Some(99)，但 nodes 中不存在 id=99
    ///   visible center=(250,150)
    ///   fallback anchor id=20 center=(250,150)，距离视口中心 0
    ///   down target id=21 center=(250,290)，Down primary=140, lateral=0, score=140
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. selected_id=99 查找失败
    ///   2. 回退到 visible center 最近的 id=20
    ///   3. 执行 Down 导航并选择 id=21
    ///
    /// 预期结果：
    ///   - 正断言：返回 Some(21)
    ///   - 负断言：不返回失效 id=99，也不返回起点 id=20
    #[test]
    fn test_navigate_card_invalid_selection_falls_back_to_visible_center_anchor() {
        let nodes = vec![node(20, 200.0, 110.0), node(21, 200.0, 250.0)];
        let visible = CanvasRect::new(0.0, 0.0, 500.0, 300.0);

        let result = navigate_card(&nodes, Some(99), visible, NavigationDirection::Down);

        assert_eq!(
            result,
            Some(21),
            "选择 id=99 已失效时应回退到视口中心附近 id=20，再向下选择 id=21"
        );
        assert_ne!(result, Some(99), "导航不应返回不存在的旧选择 id=99");
        assert_ne!(result, Some(20), "Down 导航不应返回起点自身 id=20");
    }

    /// 目标方向没有候选时返回 None，调用方应保持当前选择不变。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   selected id=30 center=(150,140)
    ///   only_left id=31 center=(10,140)，相对起点 dx=-140
    ///   执行 Right 时要求 dx>0，因此没有合法候选
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. selected_id=Some(30)，以 id=30 为起点
    ///   2. 执行 Right 导航
    ///   3. 过滤后无 dx>0 的候选
    ///
    /// 预期结果：
    ///   - 正断言：返回 None
    ///   - 负断言：不返回反方向 id=31
    #[test]
    fn test_navigate_card_returns_none_when_direction_has_no_candidate() {
        let nodes = vec![node(30, 100.0, 100.0), node(31, -40.0, 100.0)];
        let visible = CanvasRect::new(0.0, 0.0, 400.0, 320.0);

        let result = navigate_card(&nodes, Some(30), visible, NavigationDirection::Right);

        assert_eq!(
            result, None,
            "Right 方向没有合法候选时应返回 None，让调用方保持当前选择"
        );
        assert_ne!(result, Some(31), "Right 不应选择位于左侧的 id=31");
    }
}
