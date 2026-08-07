use crate::canvas::master_layout::MasterLayoutState;
use crate::motion::MotionStore;
use crate::norma::server::execute_card_command_for_area;
use canvas::{CanvasNodeKind, CanvasRect, PreviewType as CanvasPreviewType};
use norma::protocol::{
    CardCommand, CardCommandResult, PresentationDisposition, PresentationKind, PresentationLayout,
    PresentationTarget, PreviewType as NormaPreviewType,
};
use runtime::Runtime;
use std::path::PathBuf;

/// Present Browser 复用：按卡片当前 URL 精确匹配，并立即切换为 Master 且保持 Canvas 焦点。
///
/// 数据构造（含关键数值的推导过程）：
///   Browser A node_id=1，当前 URL=https://example.com/a
///   Browser B node_id=2，当前 URL=https://example.com/b
///   初始节点数 = 2；初始 Master = B；create_new=false
///   area = 1200x800，zoom=1.0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 先把 B 进入 Master Layout。
///   2. 执行 Present Browser(A)，按当前 URL 找到 node_id=1。
///   3. 复用 A、不触发新建，并把 Master 从 B 切换到 A。
///
/// 预期结果：
///   - 正断言：返回 node_id=1、Browser、Reused、Master。
///   - 正断言：Master 与唯一选择均切到 A，输入模式回到 Canvas。
///   - 负断言：节点数不得从 2 增加到 3，旧 Master B 不得继续为 Master。
#[test]
fn test_norma_present_browser_reuses_current_url_and_switches_master() {
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut master_layout = MasterLayoutState::default();
    let area = CanvasRect::new(0.0, 0.0, 1200.0, 800.0);
    let zoom = 1.0;
    let browser_a = runtime.spawn_browser(
        "https://example.com/a".to_string(),
        None,
        CanvasRect::new(0.0, 0.0, 500.0, 400.0),
    );
    let browser_b = runtime.spawn_browser(
        "https://example.com/b".to_string(),
        None,
        CanvasRect::new(520.0, 0.0, 500.0, 400.0),
    );
    motion.spawn_node(
        browser_a,
        runtime
            .document
            .node(browser_a)
            .expect("Browser A 应存在")
            .bounds,
    );
    motion.spawn_node(
        browser_b,
        runtime
            .document
            .node(browser_b)
            .expect("Browser B 应存在")
            .bounds,
    );
    assert!(
        crate::canvas::master_layout::enter_master_layout_for_area(
            &mut master_layout,
            &mut runtime,
            &mut motion,
            browser_b,
            area,
            zoom,
        ),
        "测试前置条件应让 Browser B 成为 Master"
    );
    runtime.focus.focus_node(browser_b);

    let execution = execute_card_command_for_area(
        &mut runtime,
        &mut motion,
        &mut master_layout,
        CardCommand::Present {
            target: PresentationTarget::Browser {
                url: "https://example.com/a".to_string(),
            },
            create_new: false,
        },
        area,
        zoom,
    )
    .expect("Present Browser A 应成功");

    assert_eq!(
        execution.result,
        CardCommandResult::Presented {
            node_id: browser_a,
            kind: PresentationKind::Browser,
            disposition: PresentationDisposition::Reused,
            layout: PresentationLayout::Master,
        },
        "匹配当前 URL 的 Browser A 必须如实返回 reused Master 回执"
    );
    assert_eq!(
        master_layout.master_id(),
        Some(browser_a),
        "Present 后 Browser A 必须替换 Browser B 成为 Master"
    );
    assert!(
        runtime.selection.contains(browser_a),
        "Present 目标 Browser A 必须成为唯一选择"
    );
    assert!(
        !runtime.selection.contains(browser_b),
        "旧 Master Browser B 必须降为 Detail 并取消选择"
    );
    assert_eq!(
        runtime.focus.mode(),
        runtime::InputMode::CanvasSelect,
        "Present 不得把键盘焦点留在 Browser，必须保持 Canvas 输入模式"
    );
    assert_eq!(
        runtime.document.nodes().len(),
        2,
        "默认复用不得额外创建第三张 Browser 卡片"
    );
    assert_ne!(
        master_layout.master_id(),
        Some(browser_b),
        "Present 完成后旧 Browser B 不得继续占用 Master"
    );
}

/// Present Browser 规范化复用：旧节点缺省路径与 CLI 规范化后的根路径必须视为同一 URL。
///
/// 数据构造（含关键数值的推导过程）：
///   旧 Browser node_id=1，持久化 URL=https://example.com（无尾斜杠）
///   Present target=https://example.com/（URL parser 的 canonical root）
///   初始节点数=1；create_new=false；area=1200x800；zoom=1.0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造一个升级前保存的无尾斜杠 Browser 节点。
///   2. 使用 CLI 会生成的 canonical URL 执行 Present。
///   3. 按统一 URL 规范化比较后复用旧节点并进入 Master。
///
/// 预期结果：
///   - 正断言：返回原 node_id=1、Reused、Master。
///   - 正断言：旧节点成为 Master 且唯一选中。
///   - 负断言：不得因尾斜杠差异创建第二张 Browser。
#[test]
fn test_norma_present_browser_reuses_legacy_root_url_without_trailing_slash() {
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut master_layout = MasterLayoutState::default();
    let area = CanvasRect::new(0.0, 0.0, 1200.0, 800.0);
    let zoom = 1.0;
    let browser = runtime.spawn_browser(
        "https://example.com".to_string(),
        None,
        CanvasRect::new(0.0, 0.0, 500.0, 400.0),
    );
    motion.spawn_node(
        browser,
        runtime
            .document
            .node(browser)
            .expect("旧 Browser 节点应存在")
            .bounds,
    );

    let execution = execute_card_command_for_area(
        &mut runtime,
        &mut motion,
        &mut master_layout,
        CardCommand::Present {
            target: PresentationTarget::Browser {
                url: "https://example.com/".to_string(),
            },
            create_new: false,
        },
        area,
        zoom,
    )
    .expect("canonical root URL 应复用旧 Browser");

    assert_eq!(
        execution.result,
        CardCommandResult::Presented {
            node_id: browser,
            kind: PresentationKind::Browser,
            disposition: PresentationDisposition::Reused,
            layout: PresentationLayout::Master,
        },
        "无尾斜杠旧 Browser 必须按 canonical URL 复用"
    );
    assert_eq!(
        master_layout.master_id(),
        Some(browser),
        "复用的旧 Browser 必须成为 Master"
    );
    assert!(
        runtime.selection.contains(browser),
        "复用的旧 Browser 必须成为唯一选中节点"
    );
    assert_eq!(
        runtime.document.nodes().len(),
        1,
        "尾斜杠差异不得创建第二张 Browser"
    );
    assert!(
        !matches!(
            execution.result,
            CardCommandResult::Presented {
                disposition: PresentationDisposition::Created,
                ..
            }
        ),
        "canonical URL 等价时不得返回 Created"
    );
}

/// Present Preview 副本策略：canonical path 默认复用，显式 create_new 才创建第二张卡。
///
/// 数据构造（含关键数值的推导过程）：
///   Preview path=/tmp/report.md
///   初始 Preview node_id=1；初始节点数=1
///   调用数=2：默认复用一次 + create_new=true 一次
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 对相同 path 执行默认 Present，命中 node_id=1。
///   2. 再对相同 path 执行 create_new=true，创建 node_id=2。
///   3. 检查两次回执与最终 Master。
///
/// 预期结果：
///   - 正断言：第一次 Reused node_id=1，第二次 Created node_id=2。
///   - 正断言：第二张副本成为 Master，document 最终恰有 2 个节点。
///   - 负断言：第一次调用不得创建副本，第二次不得错误复用 node_id=1。
#[test]
fn test_norma_present_preview_reuses_path_unless_new_is_explicit() {
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut master_layout = MasterLayoutState::default();
    let area = CanvasRect::new(0.0, 0.0, 1000.0, 700.0);
    let path = PathBuf::from("/tmp/report.md");
    let first_node = runtime.spawn_preview_from_path(
        path.clone(),
        Some(CanvasPreviewType::Markdown),
        CanvasRect::new(0.0, 0.0, 500.0, 400.0),
    );
    motion.spawn_node(
        first_node,
        runtime
            .document
            .node(first_node)
            .expect("初始 Preview 应存在")
            .bounds,
    );

    let reused = execute_card_command_for_area(
        &mut runtime,
        &mut motion,
        &mut master_layout,
        CardCommand::Present {
            target: PresentationTarget::Preview {
                path: path.clone(),
                explicit_type: Some(NormaPreviewType::Markdown),
            },
            create_new: false,
        },
        area,
        1.0,
    )
    .expect("相同 path 的默认 Present 应复用 Preview");
    let created = execute_card_command_for_area(
        &mut runtime,
        &mut motion,
        &mut master_layout,
        CardCommand::Present {
            target: PresentationTarget::Preview {
                path,
                explicit_type: Some(NormaPreviewType::Markdown),
            },
            create_new: true,
        },
        area,
        1.0,
    )
    .expect("显式 create_new 应创建 Preview 副本");

    assert_eq!(
        reused.result,
        CardCommandResult::Presented {
            node_id: first_node,
            kind: PresentationKind::Preview,
            disposition: PresentationDisposition::Reused,
            layout: PresentationLayout::Master,
        },
        "第一次相同路径 Present 必须复用 node_id=1"
    );
    assert_eq!(
        runtime.document.nodes().len(),
        2,
        "一次复用加一次显式副本后 document 必须恰有两张 Preview"
    );
    assert_eq!(
        created.result,
        CardCommandResult::Presented {
            node_id: 2,
            kind: PresentationKind::Preview,
            disposition: PresentationDisposition::Created,
            layout: PresentationLayout::Master,
        },
        "显式 create_new 必须创建并返回 node_id=2"
    );
    assert_eq!(
        master_layout.master_id(),
        Some(2),
        "最后展示的 Preview 副本必须成为 Master"
    );
    assert_ne!(
        created.result,
        CardCommandResult::Presented {
            node_id: first_node,
            kind: PresentationKind::Preview,
            disposition: PresentationDisposition::Reused,
            layout: PresentationLayout::Master,
        },
        "显式 create_new 不得错误复用第一张 Preview"
    );
    assert!(
        matches!(
            runtime.document.node(2).map(|node| &node.kind),
            Some(CanvasNodeKind::Preview(_))
        ),
        "显式副本 node_id=2 必须确实是 Preview，不能只是回执中的伪造 id"
    );
}
