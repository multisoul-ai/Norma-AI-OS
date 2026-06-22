use crate::master_layout::MasterLayoutState;
use crate::motion::MotionStore;
use crate::norma_server::{execute_card_command_for_area, text_for_submit, NormaCommandEffects};
use canvas::{CanvasNodeKind, CanvasRect, PreviewType as CanvasPreviewType};
use norma::protocol::{
    CardCommand, CardCommandError, CardCommandResult, CardKind, Placement,
    PreviewType as NormaPreviewType,
};
use runtime::Runtime;
use std::path::PathBuf;

/// Norma executor：创建 Terminal 应走 runtime spawn、触发布局，并标记 canvas/session dirty。
///
/// 数据构造（含关键数值的推导过程）：
///   area = 1200x900，zoom=1.0，VisibleArea 会使用 area.inset(layout_gap)
///   初始 document.nodes = 0，live_len = 0
///   Terminal shell=None，因此 executor 使用 runtime 默认 shell
///
/// 执行过程（逐步说明系统如何处理）：
///   1. execute_card_command_for_area(Create Terminal, area, zoom)
///   2. executor 调用 Runtime::spawn_terminal 创建 live node
///   3. executor 应用现有 workspace reflow，并返回 Created
///
/// 预期结果：
///   - 正断言：返回 Created{node_id=1}，document 与 live registry 都有节点
///   - 正断言：effects 同时标记 canvas_dirty/session_dirty
///   - 负断言：document 不应仍为空，effects 不应保持默认 false
#[test]
fn test_norma_create_terminal_spawns_live_node_and_marks_dirty() {
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut master_layout = MasterLayoutState::default();
    let area = CanvasRect::new(0.0, 0.0, 1200.0, 900.0);
    let zoom = 1.0;

    let execution = execute_card_command_for_area(
        &mut runtime,
        &mut motion,
        &mut master_layout,
        CardCommand::Create {
            kind: CardKind::Terminal { shell: None },
            placement: Placement::VisibleArea,
        },
        area,
        zoom,
    )
    .expect("Norma Terminal create 应执行成功");

    assert_eq!(
        execution.result,
        CardCommandResult::Created { node_id: 1 },
        "首次 Norma Terminal create 应创建 node_id=1"
    );
    assert!(
        runtime.document.node(1).is_some(),
        "创建成功后 document 应包含 node_id=1"
    );
    assert_eq!(
        runtime.live_len(),
        1,
        "Terminal create 应注册一个 live node，供后续 send 输入"
    );
    assert_eq!(
        execution.effects,
        NormaCommandEffects {
            canvas_dirty: true,
            session_dirty: true,
            notify: true,
        },
        "Terminal create 应同时标记 canvas/session dirty 并通知 UI"
    );
    assert_ne!(
        runtime.document.nodes().len(),
        0,
        "Terminal create 后 document 不应仍为空"
    );

    runtime.kill_all();
}

/// Norma executor：删除不存在节点应返回 NotFound，不能触发布局或 dirty effects。
///
/// 数据构造（含关键数值的推导过程）：
///   missing node_id = 99
///   初始 document.nodes = 0
///   area/zoom 只用于潜在 reflow；由于节点不存在，不应使用它改变布局
///
/// 执行过程（逐步说明系统如何处理）：
///   1. execute_card_command_for_area(Delete{99})
///   2. runtime.remove_node(99) 返回 false
///   3. executor 返回 CardCommandError::NotFound
///
/// 预期结果：
///   - 正断言：返回 NotFound{99}
///   - 正断言：document 仍为空
///   - 负断言：不应伪造 Deleted 成功结果
#[test]
fn test_norma_delete_missing_node_returns_not_found_without_side_effects() {
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut master_layout = MasterLayoutState::default();

    let result = execute_card_command_for_area(
        &mut runtime,
        &mut motion,
        &mut master_layout,
        CardCommand::Delete { node_id: 99 },
        CanvasRect::new(0.0, 0.0, 800.0, 600.0),
        1.0,
    );

    assert_eq!(
        result,
        Err(CardCommandError::NotFound { node_id: 99 }),
        "删除缺失 node_id=99 应返回 NotFound"
    );
    assert!(
        runtime.document.nodes().is_empty(),
        "删除缺失节点不应向 document 添加或删除其它节点"
    );
    assert_ne!(
        result,
        Ok(crate::norma_server::NormaExecution {
            result: CardCommandResult::Deleted { node_id: 99 },
            effects: NormaCommandEffects {
                canvas_dirty: true,
                session_dirty: true,
                notify: true,
            },
        }),
        "删除缺失节点不应伪造成 Deleted 成功"
    );
}

/// Norma executor：Preview 命令应创建 canvas Preview 节点，但不启动 live runtime。
///
/// 数据构造（含关键数值的推导过程）：
///   path = "/tmp/report.md"
///   explicit_type = Markdown
///   area = 800x600，zoom=1.0，VisibleArea 会使用 area.inset(layout_gap)
///   初始 document.nodes = 0，live_len = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. execute_card_command_for_area(Create Preview)
///   2. executor 调用 Runtime::spawn_preview_from_path 创建 canvas-only 节点
///   3. executor 选中节点、退出 node focus、应用布局，并返回 Created
///
/// 预期结果：
///   - 正断言：返回 Created{node_id=1}，document 中保存 Preview spec
///   - 正断言：effects 只标记 canvas_dirty，不标记 session_dirty
///   - 负断言：不应启动 Terminal live node，也不应创建非 Preview kind
#[test]
fn test_norma_preview_create_spawns_canvas_preview_without_live_node() {
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut master_layout = MasterLayoutState::default();

    let result = execute_card_command_for_area(
        &mut runtime,
        &mut motion,
        &mut master_layout,
        CardCommand::Create {
            kind: CardKind::Preview {
                path: PathBuf::from("/tmp/report.md"),
                explicit_type: Some(NormaPreviewType::Markdown),
            },
            placement: Placement::VisibleArea,
        },
        CanvasRect::new(0.0, 0.0, 800.0, 600.0),
        1.0,
    )
    .expect("Norma Preview create 应执行成功");

    assert_eq!(
        result.result,
        CardCommandResult::Created { node_id: 1 },
        "首次 Norma Preview create 应创建 node_id=1"
    );
    let node = runtime
        .document
        .node(1)
        .expect("Preview create 成功后 document 应包含 node_id=1");
    let CanvasNodeKind::Preview(spec) = &node.kind else {
        panic!(
            "Preview create 应生成 CanvasNodeKind::Preview，实际: {:?}",
            node.kind
        );
    };
    assert_eq!(
        spec.path, "/tmp/report.md",
        "Preview spec 应保留 CLI 传入的文件路径"
    );
    assert_eq!(
        spec.explicit_type,
        Some(CanvasPreviewType::Markdown),
        "Preview spec 应把 Norma PreviewType 映射到 canvas PreviewType"
    );
    assert!(
        !matches!(&node.kind, CanvasNodeKind::Term(_)),
        "Preview create 不应创建 Terminal kind 来冒充文件预览"
    );
    assert_eq!(
        runtime.live_len(),
        0,
        "Preview create 是 canvas-only 节点，不应启动 Terminal live node"
    );
    assert_eq!(
        result.effects,
        NormaCommandEffects {
            canvas_dirty: true,
            session_dirty: false,
            notify: true,
        },
        "Preview create 应只标记 canvas dirty，不应标记 terminal session dirty"
    );
}

/// Norma executor：submit 补换行只在 app 执行层发生，避免 CLI parse 阶段重复追加。
///
/// 数据构造（含关键数值的推导过程）：
///   input = "cargo test -p runtime"，长度不含换行
///   submit=true → app 执行时追加 1 个 '\n'
///   already_submitted = "pwd\n"，submit=true 时不能追加第二个换行
///
/// 执行过程（逐步说明系统如何处理）：
///   1. text_for_submit(input, true)
///   2. text_for_submit(already_submitted, true)
///   3. text_for_submit(input, false)
///
/// 预期结果：
///   - 正断言：submit=true 且无换行时补一个换行
///   - 正断言：已有换行时保持单个换行
///   - 负断言：submit=false 不应补换行
#[test]
fn test_norma_submit_text_appends_single_newline_in_executor_layer() {
    let submitted = text_for_submit("cargo test -p runtime".to_string(), true);
    let already_submitted = text_for_submit("pwd\n".to_string(), true);
    let not_submitted = text_for_submit("cargo test -p runtime".to_string(), false);

    assert_eq!(
        submitted, "cargo test -p runtime\n",
        "submit=true 应为未带换行的命令补一个换行"
    );
    assert_eq!(
        already_submitted, "pwd\n",
        "已有换行的命令不应追加第二个换行"
    );
    assert_eq!(
        not_submitted, "cargo test -p runtime",
        "submit=false 不应修改文本"
    );
    assert_ne!(
        submitted, "cargo test -p runtime\n\n",
        "submit=true 不应产生双换行导致命令执行两次"
    );
}
