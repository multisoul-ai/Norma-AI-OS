use super::*;
use canvas::{CanvasNodeKind, CanvasRect, Document, RectNode, TermSpec};

/// 场景：隐藏的桌面卡片处于终端上层。
/// 数据构造：同一 100×100 区域先放终端，再放 z_index 更高的 Rect 节点。
/// 执行过程：在 (20,20) 分别命中桌面模式和独立画布模式。
/// 预期结果：桌面命中终端，独立画布命中上层 Rect。
#[test]
fn hidden_desktop_content_does_not_take_canvas_clicks() {
    let mut document = Document::new();
    let bounds = CanvasRect::new(0.0, 0.0, 100.0, 100.0);
    let terminal = document.add_node(CanvasNodeKind::Term(TermSpec), bounds);
    let decoration = document.add_node(CanvasNodeKind::Rect(RectNode { color: 0x223344 }), bounds);
    let point = Vec2::new(20.0, 20.0);

    assert_eq!(
        visible_hit(document.nodes(), point, true, &Default::default()),
        Some(terminal),
        "桌面命中应跳过隐藏节点并落到终端"
    );
    assert_eq!(
        visible_hit(document.nodes(), point, false, &Default::default()),
        Some(decoration),
        "独立画布仍应命中顶层节点"
    );
}

/// 场景：旧终端的保存坐标位于右抽屉后面，但展示坐标已移入中央。
/// 数据构造：终端原点 x=850，展示原点 x=20，点击 (30,30)。
/// 执行过程：分别以桌面展示映射和原始节点命中。
/// 预期结果：桌面点击命中展示卡片，原始坐标不命中。
#[test]
fn presented_node_receives_click_at_visible_location() {
    let mut document = Document::new();
    let terminal = document.add_node(
        CanvasNodeKind::Term(TermSpec),
        CanvasRect::new(850.0, 20.0, 350.0, 280.0),
    );
    let point = Vec2::new(30.0, 30.0);
    let presentation =
        std::collections::HashMap::from([(terminal, CanvasRect::new(20.0, 20.0, 350.0, 280.0))]);
    assert_eq!(
        visible_hit(document.nodes(), point, true, &presentation),
        Some(terminal),
        "命中测试必须使用展示坐标"
    );
    assert_eq!(
        visible_hit(document.nodes(), point, true, &Default::default()),
        None,
        "保存坐标不可冒充展示位置"
    );
}
