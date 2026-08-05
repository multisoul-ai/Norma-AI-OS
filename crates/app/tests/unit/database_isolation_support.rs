use canvas::{
    CanvasNodeKind, CanvasNodeSnapshot, CanvasRect, DocumentSnapshot, RectNode, Viewport,
};

pub(super) fn rect_snapshot(node_id: u64, color: u32) -> DocumentSnapshot {
    DocumentSnapshot {
        nodes: vec![CanvasNodeSnapshot {
            id: node_id,
            kind: CanvasNodeKind::Rect(RectNode { color }),
            bounds: CanvasRect::new(10.0, 20.0, 100.0, 80.0),
            z_index: 1,
        }],
        viewport: Viewport::default(),
    }
}
