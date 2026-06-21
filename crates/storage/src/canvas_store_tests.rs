use super::*;
use canvas::{
    CanvasNodeSnapshot, CanvasRect, Document, DocumentSnapshot, NodeKind, RectNode, Vec2, Viewport,
};

/// C-1: CanvasStore 保存并恢复画布快照，保留节点、viewport 与 id 递增语义。
///
/// 数据构造（含关键数值的推导过程）：
///   snapshot.nodes = [id=9,z=4,bounds(10,20,300,200)]
///   viewport.pan = (5,6)，zoom = 1.25
///   恢复为 Document 后 next_id 应为 max_id(9)+1 = 10
///
/// 执行过程（逐步说明系统如何处理）：
///   1. save_snapshot(snapshot) → JSON 写入 SQLite 单例行 id=1
///   2. load_snapshot() → 读回 JSON 并反序列化
///   3. Document::from_snapshot(loaded) → 恢复 Document
///   4. add_node(Rect) → 新节点 id 应为 10
///
/// 预期结果：
///   - 正断言：读回 snapshot 与写入 snapshot 相同
///   - 正断言：新增节点 id=10
///   - 负断言：新增节点不应复用已恢复 id=9
#[test]
fn test_canvas_snapshot_round_trips_and_preserves_next_node_id() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let store = CanvasStore::new(temp.path().join("project.sqlite"));
    let snapshot = DocumentSnapshot {
        nodes: vec![CanvasNodeSnapshot {
            id: 9,
            kind: NodeKind::Rect(RectNode { color: 0x35d07f }),
            bounds: CanvasRect::new(10.0, 20.0, 300.0, 200.0),
            z_index: 4,
        }],
        viewport: Viewport {
            pan: Vec2::new(5.0, 6.0),
            zoom: 1.25,
        },
    };

    store
        .save_snapshot(&snapshot)
        .expect("canvas snapshot 应写入 SQLite");
    let loaded = store
        .load_snapshot()
        .expect("canvas snapshot 应可读取")
        .expect("canvas snapshot 应存在");
    let mut document = Document::from_snapshot(loaded.clone());
    let next_id = document.add_node(
        NodeKind::Rect(RectNode { color: 0x4080ff }),
        CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );

    assert_eq!(
        loaded, snapshot,
        "CanvasStore 应完整保留 DocumentSnapshot 内容"
    );
    assert_eq!(next_id, 10, "恢复 max_id=9 后新增节点 id 应为 10");
    assert_ne!(next_id, 9, "新增节点不应复用已持久化节点 id=9");
}
