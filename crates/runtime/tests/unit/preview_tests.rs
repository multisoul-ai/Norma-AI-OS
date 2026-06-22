use super::*;
use canvas::{CanvasNodeKind, CanvasRect, NodeType, PreviewType};

/// Preview spawn 只创建 canvas 描述节点，不注册 live node，也不产生 session checkpoint。
///
/// 数据构造（含关键数值推导）：
///   bounds = CanvasRect::new(12, 24, 640, 420)
///   path = "/tmp/preview.md"，explicit_type = Some(Markdown)
///   document.next_id 初始为 1，因此 spawn_preview_from_path 返回 node_id=1
///   Preview 不是 live PTY，因此 live_len 应保持 0，node_caps(1) 应为 Caps::NONE
///
/// 执行过程：
///   1. Runtime::new()
///   2. spawn_preview_from_path(path, Some(Markdown), bounds)
///      → document.add_node(NodeKind::Preview(...), bounds)
///      → 不调用 TermLive::spawn_command，不写 live map
///   3. 查询 document、caps、node_view、session checkpoints
///
/// 预期结果：
///   - 正断言：document 中存在 Preview 节点，path 和 explicit_type 完整保存
///   - 正断言：Preview caps 为 Caps::NONE，live_len 为 0
///   - 负断言：node_view 不应存在，session checkpoint 不应包含 Preview
#[test]
fn test_spawn_preview_creates_document_only_node_without_live_session() {
    let mut runtime = Runtime::new();
    let bounds = CanvasRect::new(12.0, 24.0, 640.0, 420.0);
    let path = std::path::PathBuf::from("/tmp/preview.md");

    let node_id =
        runtime.spawn_preview_from_path(path.clone(), Some(PreviewType::Markdown), bounds);

    let node = runtime
        .document
        .node(node_id)
        .expect("spawn_preview_from_path 返回的 node_id 应存在于 document");
    assert_eq!(
        node.kind.node_type(),
        NodeType::Preview,
        "spawn_preview_from_path 应创建 Preview canvas node"
    );
    match &node.kind {
        CanvasNodeKind::Preview(spec) => {
            assert_eq!(
                spec.path,
                path.to_string_lossy(),
                "PreviewSpec.path 应保存传入路径，供重开 workspace 后重新读取"
            );
            assert_eq!(
                spec.explicit_type,
                Some(PreviewType::Markdown),
                "PreviewSpec.explicit_type 应保存显式 Markdown 类型"
            );
        }
        CanvasNodeKind::Term(_) => panic!("Preview spawn 不应创建 Terminal 节点"),
        CanvasNodeKind::Agent(_) => panic!("Preview spawn 不应创建 Agent 节点"),
        CanvasNodeKind::Rect(_) => panic!("Preview spawn 不应创建 Rect 节点"),
    }
    assert_eq!(
        runtime.live_len(),
        0,
        "Preview 不应注册 live node，否则会被误当成可运行 session"
    );
    assert_eq!(
        runtime.node_caps(node_id),
        Caps::NONE,
        "Preview 不接收键盘输入，也不需要 terminal resize"
    );
    assert!(
        runtime.node_view(node_id).is_none(),
        "Preview 没有 live view，内容读取由 app 层 Preview cache 负责"
    );
    assert!(
        runtime.collect_session_checkpoints().is_empty(),
        "Preview 不应产生 session checkpoint，否则恢复时会被误当成 live session"
    );
}
