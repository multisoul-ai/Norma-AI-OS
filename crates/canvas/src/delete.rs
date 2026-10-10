//! 删除相关的纯逻辑：计算删除后应承接选择的邻居节点。
//!
//! 零 gpui，可与 document/selection 一起独立 `cargo test -p canvas`。

use crate::node::{CanvasNode, NodeId};

/// 删除 deleted 后应承接选择的邻居节点 id。
///
/// nodes 必须是删除前的顺序（`document.nodes()` 的插入序，与 reflow 行主序一致）。
/// 取被删节点在 nodes 中的下标 i：
///   - 优先返回 `i+1`（删除后该节点滑入槽位 i，视觉上“补位”）；
///   - 若被删节点位于末尾（无后继），回退到前驱 `i-1`；
///   - 列表为空、被删 id 不存在、或列表仅含被删节点本身时返回 None。
///
/// `Document::remove_node` 用 `Vec::remove(index)` 删节点，保留幸存者的相对顺序，
/// 因此删除前的 `i+1` 正是删除后落到下标 i 的节点。
pub fn next_neighbor_after_delete(nodes: &[CanvasNode], deleted: NodeId) -> Option<NodeId> {
    let i = nodes.iter().position(|n| n.id == deleted)?;
    if let Some(next) = nodes.get(i + 1) {
        Some(next.id)
    } else if i > 0 {
        Some(nodes[i - 1].id)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::CanvasRect;
    use crate::node::{CanvasNodeKind, RectNode};

    /// 构造给定 id 序列的 Rect 节点列表，供 next_neighbor_after_delete 测试。
    /// bounds 用递增 x 原点，使插入序与 id 升序一致（add_node 亦按此序分配）。
    fn make_test_nodes(ids: &[NodeId]) -> Vec<CanvasNode> {
        ids.iter()
            .enumerate()
            .map(|(i, &id)| CanvasNode {
                card_instance_id: crate::new_card_instance_id(),
                id,
                kind: CanvasNodeKind::Rect(RectNode { color: 0x808080 }),
                bounds: CanvasRect::new(i as f32 * 200.0, 0.0, 100.0, 80.0),
                z_index: i as i32,
                selected: false,
            })
            .collect()
    }

    /// 删除中间节点：应返回后继邻居（删除后滑入被删槽位）。
    ///
    /// 数据构造（id 自 1 起单调递增，见 Document::new 的 next_id=1）：
    ///   nodes = [1, 2, 3, 4]，删除 id=2（下标 i=1）。
    ///
    /// 执行过程：
    ///   1. next_neighbor_after_delete(&nodes, 2) → 取下标 i=1
    ///   2. 命中 i+1=2 的节点 id=3 → 返回 Some(3)
    ///
    /// 预期结果：
    ///   - 正断言：返回 Some(3)（i+1 节点，将滑入槽位 1）
    ///   - 负断言：不返回 Some(2)（不得返回被删 id 自身）
    ///   - 负断言：不返回 None（确保非空列表、命中 id 时必有邻居）
    #[test]
    fn test_next_neighbor_after_delete_middle_returns_next() {
        let nodes = make_test_nodes(&[1, 2, 3, 4]);
        let result = next_neighbor_after_delete(&nodes, 2);
        assert_eq!(
            result,
            Some(3),
            "删除 id=2（下标1）应返回后继 id=3（滑入槽位1）"
        );
        assert!(
            result != Some(2),
            "不得返回被删除的 id 自身，否则后续 Cmd+D 会面对已删除节点"
        );
        assert!(result.is_some(), "列表非空且命中 id 时必须返回一个邻居");
    }

    /// 删除末尾节点：无后继，应回退到前驱。
    ///
    /// 数据构造：nodes = [1, 2, 3, 4]，删除 id=4（下标 i=3，末尾）。
    ///
    /// 执行过程：
    ///   1. next_neighbor_after_delete(&nodes, 4) → 取下标 i=3
    ///   2. i+1 越界 → 回退 i-1=2 的节点 id=3 → 返回 Some(3)
    ///
    /// 预期结果：
    ///   - 正断言：返回 Some(3)（前驱，下标 i-1）
    ///   - 负断言：不返回 Some(4)（不得返回被删 id）
    ///   - 负断言：不返回 None
    #[test]
    fn test_next_neighbor_after_delete_last_returns_previous() {
        let nodes = make_test_nodes(&[1, 2, 3, 4]);
        let result = next_neighbor_after_delete(&nodes, 4);
        assert_eq!(
            result,
            Some(3),
            "删除末尾 id=4 应回退到前驱 id=3（下标 i-1=2）"
        );
        assert!(
            result != Some(4),
            "不得返回被删除的 id 自身，已删除节点不能承接选择"
        );
        assert!(result.is_some(), "末尾删除但有前驱时必须返回邻居");
    }

    /// 删除首个节点：应返回原第二个（滑入槽位 0）。
    ///
    /// 数据构造：nodes = [1, 2, 3, 4]，删除 id=1（下标 i=0）。
    ///
    /// 执行过程：
    ///   1. next_neighbor_after_delete(&nodes, 1) → 取下标 i=0
    ///   2. 命中 i+1=1 的节点 id=2 → 返回 Some(2)
    ///
    /// 预期结果：
    ///   - 正断言：返回 Some(2)（i+1 节点，滑入槽位 0）
    ///   - 负断言：不返回 Some(1)
    ///   - 负断言：不返回 None
    #[test]
    fn test_next_neighbor_after_delete_first_returns_second() {
        let nodes = make_test_nodes(&[1, 2, 3, 4]);
        let result = next_neighbor_after_delete(&nodes, 1);
        assert_eq!(
            result,
            Some(2),
            "删除首个 id=1 应返回原第二个 id=2（滑入槽位0）"
        );
        assert!(
            result != Some(1),
            "不得返回被删除的 id 自身，否则选择集指向已删除节点"
        );
        assert!(result.is_some(), "首个删除但有后继时必须返回邻居");
    }

    /// 列表仅含被删节点：返回 None（无幸存者，选择集应空）。
    ///
    /// 数据构造：nodes = [1]，删除 id=1（下标 i=0，唯一且为首）。
    ///
    /// 执行过程：
    ///   1. next_neighbor_after_delete(&nodes, 1) → 取下标 i=0
    ///   2. i+1 越界，i>0 不成立 → 返回 None
    ///
    /// 预期结果：
    ///   - 正断言：返回 None（无邻居可承接）
    #[test]
    fn test_next_neighbor_after_delete_only_node_returns_none() {
        let nodes = make_test_nodes(&[1]);
        let result = next_neighbor_after_delete(&nodes, 1);
        assert!(
            result.is_none(),
            "列表仅含被删节点时返回 None，调用方应清空选择"
        );
    }

    /// 被删 id 不在列表中：返回 None（安全回退，不应 panic）。
    ///
    /// 数据构造：nodes = [1, 2, 3]，删除 id=99（不存在）。
    ///
    /// 执行过程：
    ///   1. next_neighbor_after_delete(&nodes, 99) → position 未命中 → 提前返回 None
    ///
    /// 预期结果：
    ///   - 正断言：返回 None
    #[test]
    fn test_next_neighbor_after_delete_unknown_id_returns_none() {
        let nodes = make_test_nodes(&[1, 2, 3]);
        let result = next_neighbor_after_delete(&nodes, 99);
        assert!(result.is_none(), "被删 id 不在列表时应返回 None 而非 panic");
    }
}
