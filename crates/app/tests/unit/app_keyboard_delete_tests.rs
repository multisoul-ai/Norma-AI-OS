use super::*;
use canvas::{CanvasNodeKind, CanvasRect, RectNode};
use runtime::Runtime;

fn add_rect(runtime: &mut Runtime, x: f32, color: u32) -> NodeId {
    runtime.document.add_node(
        CanvasNodeKind::Rect(RectNode { color }),
        CanvasRect::new(x, 0.0, 100.0, 80.0),
    )
}

/// Delete Card 目标解析：聚焦卡片应优先于旧的画布选择。
///
/// 数据构造（含关键数值的推导过程）：
///   A node_id = 1（Document 首个节点）
///   B node_id = 2（Document 第二个节点）
///   selection = {1}
///   focus = NodeFocused(2)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 A/B 两张普通卡片
///   2. selection.select_only(A) → 画布仍保留旧选中 A
///   3. focus.focus_node(B) → 用户当前正在 B 卡片内操作
///   4. current_delete_node_id(runtime) → 应选择当前聚焦的 B
///
/// 预期结果：
///   - 正断言：返回 Some(B)，Cmd+D 删除当前聚焦卡片
///   - 负断言：不返回 Some(A)，避免删除旧 selection 指向的卡片
#[test]
fn test_current_delete_node_id_prefers_focused_card_over_selection() {
    let mut runtime = Runtime::new();
    let a = add_rect(&mut runtime, 0.0, 0x4080ff);
    let b = add_rect(&mut runtime, 120.0, 0xff8040);
    runtime.selection.select_only(a);
    runtime.focus.focus_node(b);

    let target = current_delete_node_id(&runtime);

    assert_eq!(target, Some(b), "Delete Card 应优先删除当前聚焦的 B 卡片");
    assert_ne!(
        target,
        Some(a),
        "Delete Card 不应因旧 selection 仍指向 A 而误删 A"
    );
}

/// Delete Card 目标解析：无聚焦卡片时，单选且存在的画布卡片可作为删除目标。
///
/// 数据构造（含关键数值的推导过程）：
///   A node_id = 1
///   B node_id = 2
///   focus = CanvasSelect（Runtime 默认输入模式）
///   selection = {2}
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 A/B 两张普通卡片
///   2. 只选中 B，且不聚焦任何节点
///   3. current_delete_node_id(runtime) → 应回退到单选 B
///
/// 预期结果：
///   - 正断言：返回 Some(B)，Cmd+D 可删除单选卡片
///   - 负断言：不返回 Some(A)，未选中的卡片不能被删除
#[test]
fn test_current_delete_node_id_uses_single_selected_card_without_focus() {
    let mut runtime = Runtime::new();
    let a = add_rect(&mut runtime, 0.0, 0x4080ff);
    let b = add_rect(&mut runtime, 120.0, 0xff8040);
    runtime.selection.select_only(b);

    let target = current_delete_node_id(&runtime);

    assert_eq!(
        target,
        Some(b),
        "无聚焦卡片时 Delete Card 应删除唯一选中的 B 卡片"
    );
    assert_ne!(target, Some(a), "Delete Card 不应删除未选中的 A 卡片");
}

/// Delete Card 目标解析：失效聚焦不应阻止回退到单选存在卡片。
///
/// 数据构造（含关键数值的推导过程）：
///   A node_id = 1
///   stale focused id = 99（document 中不存在）
///   selection = {1}
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 A 卡片并选中
///   2. focus.focus_node(99) 模拟运行时残留的失效焦点
///   3. current_delete_node_id(runtime) → 应忽略失效焦点并回退到 A
///
/// 预期结果：
///   - 正断言：返回 Some(A)，存在的单选卡片仍可删除
///   - 负断言：不返回 Some(99)，不存在的焦点不能成为删除目标
#[test]
fn test_current_delete_node_id_falls_back_when_focus_is_stale() {
    let mut runtime = Runtime::new();
    let a = add_rect(&mut runtime, 0.0, 0x4080ff);
    runtime.selection.select_only(a);
    runtime.focus.focus_node(99);

    let target = current_delete_node_id(&runtime);

    assert_eq!(
        target,
        Some(a),
        "失效聚焦 id=99 不存在时 Delete Card 应回退删除单选 A"
    );
    assert_ne!(
        target,
        Some(99),
        "Delete Card 不应返回 document 中不存在的失效焦点 id=99"
    );
}

/// Delete Card 目标解析：多选、失效选择和空选择都没有确定删除目标。
///
/// 数据构造（含关键数值的推导过程）：
///   A node_id = 1，B node_id = 2
///   multi selection = {1,2}
///   stale selection = {99}，但 document 中不存在 id=99
///   empty selection = {}
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 多选 A+B → current_delete_node_id 返回 None
///   2. 只选失效 id=99 → current_delete_node_id 返回 None
///   3. 清空选择 → current_delete_node_id 返回 None
///
/// 预期结果：
///   - 正断言：三种不明确状态都返回 None
///   - 负断言：不应任选 A、B 或不存在的 99 作为删除目标
#[test]
fn test_current_delete_node_id_rejects_multi_stale_and_empty_selection() {
    let mut runtime = Runtime::new();
    let a = add_rect(&mut runtime, 0.0, 0x4080ff);
    let b = add_rect(&mut runtime, 120.0, 0xff8040);

    runtime.selection.select_only(a);
    runtime.selection.add(b);
    let multi = current_delete_node_id(&runtime);

    runtime.selection.clear();
    runtime.selection.select_only(99);
    let stale = current_delete_node_id(&runtime);

    runtime.selection.clear();
    let empty = current_delete_node_id(&runtime);

    assert_eq!(multi, None, "多选时 Delete Card 不应选择删除目标");
    assert_ne!(multi, Some(a), "多选时不应默认删除 A");
    assert_ne!(multi, Some(b), "多选时不应默认删除 B");
    assert_eq!(
        stale, None,
        "选择 id=99 不存在时 Delete Card 不应选择删除目标"
    );
    assert_ne!(
        stale,
        Some(99),
        "Delete Card 不应返回 document 中不存在的失效选择 id=99"
    );
    assert_eq!(empty, None, "空选择且无聚焦时 Delete Card 应静默无目标");
}
