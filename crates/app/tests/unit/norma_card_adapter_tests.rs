use super::execute_norma_adapter_command;
use crate::preview::{PreviewContent, PreviewLoadState};
use canvas::CanvasNodeKind;
use gpui::TestAppContext;
use std::path::PathBuf;
use terminal::{PtyBackend, TestPtyBackend};

fn echo_factory() -> runtime::PtyFactory {
    Box::new(|_cmd, _cols, _rows, tx| Ok(Box::new(TestPtyBackend::echo(tx)) as Box<dyn PtyBackend>))
}

/// 生产 Adapter 应把 Norma 的 Browser 与 Preview 工具落为真实画布卡片。
///
/// 数据构造（含关键数值的推导过程）：
///   Initial nodes = 0
///   Create commands = 2（Browser + Preview）
///   Expected nodes = 0 + 2 = 2
///   Expected live PTY nodes = 0（两种卡片均为 canvas-only）
///   Browser URL = None → 产品默认首页 `https://www.google.com`
///   Preview path = `docs/report.md`，explicit type = Markdown
///   Preview source = `# Report`，8 bytes（1 + 1 + 6）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless MultisoulApp 并进入隔离工作区 Canvas。
///   2. 通过生产 `execute_norma_adapter_command` 创建默认 Browser 卡片。
///   3. 通过同一 Adapter 创建 Markdown Preview 卡片。
///   4. 推进后台文件读取，再读取 Runtime 与 PreviewStore 状态。
///
/// 预期结果：
///   - 正断言：两个回执分别返回 node_id=1 与 node_id=2，document 保存准确 spec。
///   - 正断言：后创建的 Preview 成为唯一选中卡片并加载为 Markdown Ready。
///   - 负断言：不得停在 Error、创建 Terminal/Agent 节点或注册任何 live PTY。
#[test]
fn test_norma_adapter_creates_browser_and_preview_canvas_cards() {
    let mut gpui = TestAppContext::single();
    let workspace =
        tempfile::TempDir::new().expect("Norma card temp workspace should be creatable");
    std::fs::create_dir(workspace.path().join("docs"))
        .expect("Preview fixture directory should be creatable");
    std::fs::write(workspace.path().join("docs/report.md"), "# Report")
        .expect("Preview fixture should be writable");
    let (app, cx) = gpui.add_window_view(|_window, cx| {
        crate::app::MultisoulApp::new_with_pty_factory(cx, echo_factory())
    });
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });

    let browser = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            execute_norma_adapter_command(
                app,
                window,
                cx,
                agent::AdapterCommand::CreateBrowser { url: None },
            )
        })
    });
    let preview = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            execute_norma_adapter_command(
                app,
                window,
                cx,
                agent::AdapterCommand::CreatePreview {
                    path: PathBuf::from("docs/report.md"),
                    explicit_type: Some(norma::protocol::PreviewType::Markdown),
                },
            )
        })
    });
    cx.run_until_parked();

    let browser = browser.expect("create_browser should execute on the production App Adapter");
    let preview = preview.expect("create_preview should execute on the production App Adapter");
    assert_eq!(
        browser.node_id,
        Some(1),
        "the first Norma-created Browser should return node_id=1"
    );
    assert_eq!(
        preview.node_id,
        Some(2),
        "the second Norma-created Preview should return node_id=2"
    );
    assert_eq!(
        browser.status, "succeeded",
        "Browser receipt should report the completed canvas side effect"
    );
    assert_eq!(
        preview.status, "succeeded",
        "Preview receipt should report the completed canvas side effect"
    );

    let (nodes, selected, live_len, preview_state) = app.read_with(cx, |app, _cx| {
        (
            app.runtime.document.nodes().to_vec(),
            app.runtime.selection.iter().collect::<Vec<_>>(),
            app.runtime.live_len(),
            app.previews.state_for(2).cloned(),
        )
    });
    assert_eq!(
        nodes.len(),
        2,
        "two Norma create tools should add exactly two nodes"
    );
    let CanvasNodeKind::Browser(browser_spec) = &nodes[0].kind else {
        panic!("node 1 should be Browser, actual kind: {:?}", nodes[0].kind);
    };
    assert_eq!(
        browser_spec.url, "https://www.google.com",
        "omitted Browser URL should use the product default home page"
    );
    let CanvasNodeKind::Preview(preview_spec) = &nodes[1].kind else {
        panic!("node 2 should be Preview, actual kind: {:?}", nodes[1].kind);
    };
    assert_eq!(
        PathBuf::from(&preview_spec.path),
        workspace.path().join("docs/report.md"),
        "Preview node should resolve a workspace-relative path against the current project"
    );
    assert_eq!(
        preview_spec.explicit_type,
        Some(canvas::PreviewType::Markdown),
        "Preview node should preserve the explicit Markdown renderer"
    );
    assert_eq!(
        selected,
        vec![2],
        "the latest Norma-created Preview should be the only selected card"
    );
    assert_eq!(
        live_len, 0,
        "Browser and Preview cards must not register Terminal-backed live nodes"
    );
    let Some(PreviewLoadState::Ready(PreviewContent::Markdown(document))) = preview_state else {
        panic!("Preview should finish in Markdown Ready state, actual: {preview_state:?}");
    };
    assert_eq!(
        document.source, "# Report",
        "Preview should load the requested workspace Markdown file"
    );
    assert!(
        !document.source.is_empty(),
        "Preview Ready content must not be an empty fallback after a path error"
    );
    assert!(
        !nodes.iter().any(|node| matches!(
            node.kind,
            CanvasNodeKind::Term(_) | CanvasNodeKind::Agent(_)
        )),
        "Browser and Preview tools must not create Terminal or Agent nodes"
    );
}

/// Norma Adapter：按 1-based 画布位置关闭卡片时只删除对应顺序的节点。
///
/// 数据构造（含关键数值的推导过程）：
///   初始卡片数          = 0
///   创建 Browser 数     = 3
///   文档顺序            = [node 1, node 2, node 3]
///   删除位置            = 2（1-based）→ node 2
///   删除后卡片数         = 3 - 1 = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless App 并进入隔离工作区 Canvas。
///   2. 通过生产 Adapter 顺序创建三张 Browser 卡片。
///   3. 执行 `DeleteCanvasCardAtPosition { position: 2 }`。
///   4. 读取回执和 Runtime document 的三个确定节点。
///
/// 预期结果：
///   - 正断言：回执成功且返回实际删除的 node_id=2。
///   - 正断言：第一张 node 1 与第三张 node 3 分别保留。
///   - 负断言：第二张 node 2 不再存在，且不得误删相邻卡片。
#[test]
fn test_norma_adapter_deletes_only_card_at_position() {
    let mut gpui = TestAppContext::single();
    let workspace =
        tempfile::TempDir::new().expect("position delete temp workspace should be creatable");
    let (app, cx) = gpui.add_window_view(|_window, cx| {
        crate::app::MultisoulApp::new_with_pty_factory(cx, echo_factory())
    });
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });

    for url in [
        "https://first.example",
        "https://second.example",
        "https://third.example",
    ] {
        cx.update(|window, app_cx| {
            app.update(app_cx, |app, cx| {
                execute_norma_adapter_command(
                    app,
                    window,
                    cx,
                    agent::AdapterCommand::CreateBrowser {
                        url: Some(url.to_string()),
                    },
                )
            })
        })
        .expect("each Browser fixture should be created through the production Adapter");
    }

    let deleted = cx
        .update(|window, app_cx| {
            app.update(app_cx, |app, cx| {
                execute_norma_adapter_command(
                    app,
                    window,
                    cx,
                    agent::AdapterCommand::DeleteCanvasCardAtPosition { position: 2 },
                )
            })
        })
        .expect("position=2 should resolve and delete the second Browser card");
    let (node_1_exists, node_2_exists, node_3_exists, remaining) = app.read_with(cx, |app, _cx| {
        (
            app.runtime.document.node(1).is_some(),
            app.runtime.document.node(2).is_some(),
            app.runtime.document.node(3).is_some(),
            app.runtime.document.nodes().len(),
        )
    });

    assert_eq!(
        deleted.status, "succeeded",
        "position delete should report the production Adapter's completed side effect"
    );
    assert_eq!(
        deleted.node_id,
        Some(2),
        "position=2 must return the actual deleted node_id=2"
    );
    assert!(
        node_1_exists,
        "position=1 / node 1 must remain after deleting position=2"
    );
    assert!(
        !node_2_exists,
        "position=2 / node 2 must be absent after the successful delete"
    );
    assert!(
        node_3_exists,
        "position=3 / node 3 must remain after deleting position=2"
    );
    assert_eq!(
        remaining, 2,
        "deleting one of three cards must leave exactly two cards"
    );
}

/// Norma Adapter：越界画布位置必须安全拒绝且不删除任何卡片。
///
/// 数据构造（含关键数值的推导过程）：
///   初始卡片数          = 0
///   创建 Browser 数     = 2
///   有效位置范围         = 1..=2
///   请求位置            = 3 → 超出最大位置 2
///   拒绝后卡片数         = 2 - 0 = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless App 并进入隔离工作区 Canvas。
///   2. 通过生产 Adapter 顺序创建 node 1 与 node 2。
///   3. 执行 `DeleteCanvasCardAtPosition { position: 3 }`。
///   4. 读取错误与 Runtime document 的确定节点。
///
/// 预期结果：
///   - 负断言：position=3 被明确拒绝，不返回成功回执。
///   - 正断言：node 1 与 node 2 都继续存在，卡片数仍为 2。
///   - 负断言：越界位置不得退化为删除最后一张或其他任意卡片。
#[test]
fn test_norma_adapter_rejects_out_of_range_position_without_deleting() {
    let mut gpui = TestAppContext::single();
    let workspace =
        tempfile::TempDir::new().expect("position rejection temp workspace should be creatable");
    let (app, cx) = gpui.add_window_view(|_window, cx| {
        crate::app::MultisoulApp::new_with_pty_factory(cx, echo_factory())
    });
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });

    for url in ["https://first.example", "https://second.example"] {
        cx.update(|window, app_cx| {
            app.update(app_cx, |app, cx| {
                execute_norma_adapter_command(
                    app,
                    window,
                    cx,
                    agent::AdapterCommand::CreateBrowser {
                        url: Some(url.to_string()),
                    },
                )
            })
        })
        .expect("each Browser fixture should be created before the out-of-range request");
    }

    let rejected = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            execute_norma_adapter_command(
                app,
                window,
                cx,
                agent::AdapterCommand::DeleteCanvasCardAtPosition { position: 3 },
            )
        })
    });
    let (node_1_exists, node_2_exists, remaining) = app.read_with(cx, |app, _cx| {
        (
            app.runtime.document.node(1).is_some(),
            app.runtime.document.node(2).is_some(),
            app.runtime.document.nodes().len(),
        )
    });

    assert!(
        matches!(rejected, Err(agent::AdapterError::Rejected(_))),
        "position=3 on a two-card Canvas must be rejected; actual: {rejected:?}"
    );
    assert!(
        node_1_exists,
        "out-of-range position must preserve the first Canvas card"
    );
    assert!(
        node_2_exists,
        "out-of-range position must preserve the second Canvas card"
    );
    assert_eq!(
        remaining, 2,
        "rejecting an out-of-range close must leave both original cards"
    );
}
