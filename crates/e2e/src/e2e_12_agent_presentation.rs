use app::MultisoulApp;
use canvas::CanvasNodeKind;
use gpui::{Entity, TestAppContext, VisualTestContext};
use norma::protocol::{
    AgentProfile, CardCommand, CardCommandResult, PresentationDisposition, PresentationKind,
    PresentationLayout, PresentationTarget, ProviderAuthorization, WireRequest,
};

use crate::harness::{echo_factory, enter_canvas, make_app, read_runtime};

/// Agent 内容展示闭环：受权来源连续展示 Preview/Browser，复用目标并进入 Master Layout。
///
/// 数据构造（含关键数值的推导过程）：
///   来源 Agent = Alan/Codex，节点数初始=1
///   Present 调用数=3：Preview(created) + Browser(created) + 同 URL Browser(reused)
///   最终内容卡数=2：Preview 1 + Browser 1；总节点数=1+2=3
///   同来源通知计数=3；单轮上限=3
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入 Canvas、安装测试授权并激活 Alan/Codex。
///   2. 通过生产 WireRequest 先展示 Markdown Preview。
///   3. 展示 Browser，再展示完全相同的当前 URL。
///   4. 推进 GPUI 后台读取与渲染，读取运行时、布局和通知投影。
///
/// 预期结果：
///   - 正断言：前两次 Created，第三次 Reused 且三次均确认 Master。
///   - 正断言：最终只有一张 Browser 和一张 Preview，Browser 为 Master。
///   - 正断言：Preview 后台读取完成，Canvas 保持焦点，通知显示 Alan×3。
///   - 负断言：复用不得创建第二张 Browser，Preview 不得继续占用 Master。
#[test]
fn e2e_12_agent_presents_preview_and_reuses_browser_in_master_layout() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("展示 e2e 应创建隔离 workspace");
    let preview_path = workspace.path().join("review.md");
    std::fs::write(&preview_path, "# Review\n\nReady.").expect("展示 e2e 应写入 Markdown fixture");
    let preview_path =
        std::fs::canonicalize(preview_path).expect("Markdown fixture 应可 canonicalize");
    enter_canvas(&app, cx, workspace.path());
    app.update(cx, |app, _cx| {
        app.install_presentation_authorization_for_e2e(
            workspace.path().join("authorization.sqlite"),
            "codex",
            "codex-presentation-token",
        )
        .expect("展示 e2e 应安装 Codex 测试授权");
    });
    let source_node_id = create_source_agent(&app, cx);

    let (preview, browser, reused) = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            let preview = execute_present(
                app,
                window,
                cx,
                21,
                source_node_id,
                PresentationTarget::Preview {
                    path: preview_path,
                    explicit_type: None,
                },
            );
            let browser = execute_present(
                app,
                window,
                cx,
                22,
                source_node_id,
                PresentationTarget::Browser {
                    url: "https://example.com/review".to_string(),
                },
            );
            let reused = execute_present(
                app,
                window,
                cx,
                23,
                source_node_id,
                PresentationTarget::Browser {
                    url: "https://example.com/review".to_string(),
                },
            );
            (preview, browser, reused)
        })
    });
    cx.run_until_parked();

    let snapshot = app.read_with(cx, |app, _cx| app.presentation_e2e_snapshot());
    let (preview_nodes, browser_nodes) = read_runtime(&app, cx, |runtime| {
        let previews = runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Preview(_)))
            .map(|node| (node.id, node.bounds))
            .collect::<Vec<_>>();
        let browsers = runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Browser(_)))
            .map(|node| (node.id, node.bounds))
            .collect::<Vec<_>>();
        (previews, browsers)
    });

    assert_presented(
        &preview.result,
        PresentationKind::Preview,
        PresentationDisposition::Created,
        "首次 Preview",
    );
    assert_presented(
        &browser.result,
        PresentationKind::Browser,
        PresentationDisposition::Created,
        "首次 Browser",
    );
    assert_presented(
        &reused.result,
        PresentationKind::Browser,
        PresentationDisposition::Reused,
        "相同 URL Browser",
    );
    assert_eq!(
        preview_nodes.len(),
        1,
        "三次展示后必须只有一张 Preview，不能重复创建"
    );
    assert_eq!(
        browser_nodes.len(),
        1,
        "相同当前 URL 的第三次展示必须复用，不能创建第二张 Browser"
    );
    assert_eq!(
        snapshot.master_id,
        Some(browser_nodes[0].0),
        "最后展示的复用 Browser 必须保持为 Master"
    );
    assert!(
        browser_nodes[0].1.size.x > preview_nodes[0].1.size.x,
        "Browser Master 宽度必须大于 Preview Detail"
    );
    assert_eq!(
        snapshot.preview_states,
        vec![(preview_nodes[0].0, "ready")],
        "推进后台任务后 Markdown Preview 必须读取为 Ready"
    );
    assert!(
        snapshot.canvas_focused,
        "Agent 主动展示后不得把键盘焦点交给 Browser 或 Preview"
    );
    assert_eq!(
        snapshot.notification_source.as_deref(),
        Some("Alan"),
        "通知必须标注真实来源 Agent Alan"
    );
    assert_eq!(
        snapshot.notification_count, 3,
        "同一 Agent 连续三次展示必须合并为一条 count=3 通知"
    );
    assert!(
        cx.debug_bounds("presentation-notification").is_some(),
        "展示通知必须渲染稳定 debug selector 供真实页面验收"
    );
    assert_ne!(
        snapshot.master_id,
        Some(preview_nodes[0].0),
        "Browser 展示后旧 Preview 不得继续占用 Master"
    );
}

/// Browser 导航复用闭环：A 导航到 B 后只复用 B，再展示 A 必须创建新卡。
///
/// 数据构造（含关键数值的推导过程）：
///   来源 Agent=Alan/Codex；Present 调用数=3，符合单轮上限
///   调用顺序=A(created) → B(reused after native location event) → A(created)
///   最终 Browser 数=2：原 node 当前 URL=B + 新 node URL=A
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入 Canvas、安装 Codex 授权并激活来源 Agent。
///   2. Present A 创建第一张 Browser。
///   3. 注入生产 Browser LocationChanged(B) 事件并走正式 drain/持久化路径。
///   4. Present B 应复用第一张；Present A 应创建第二张并成为 Master。
///
/// 预期结果：
///   - 正断言：B 返回原 node_id 与 Reused，证明按真实当前 URL 匹配。
///   - 正断言：再次 A 返回新 node_id 与 Created，并成为 Master。
///   - 负断言：导航后不得继续把原节点错误匹配为 A。
#[test]
fn e2e_12_browser_navigation_updates_presentation_reuse_target() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("导航复用 e2e 应创建隔离 workspace");
    enter_canvas(&app, cx, workspace.path());
    app.update(cx, |app, _cx| {
        app.install_presentation_authorization_for_e2e(
            workspace.path().join("authorization.sqlite"),
            "codex",
            "codex-presentation-token",
        )
        .expect("导航复用 e2e 应安装 Codex 测试授权");
    });
    let source_node_id = create_source_agent(&app, cx);

    let first = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            execute_present(
                app,
                window,
                cx,
                31,
                source_node_id,
                PresentationTarget::Browser {
                    url: "https://example.com/a".to_string(),
                },
            )
        })
    });
    let first_node_id = presented_node_id(&first.result, "首次 A");
    app.update(cx, |app, _cx| {
        assert!(
            app.report_browser_location_for_e2e(first_node_id, "https://example.com/b".to_string()),
            "native LocationChanged(B) 必须更新 Browser 当前地址"
        );
    });

    let (current_b, old_a) = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            let current_b = execute_present(
                app,
                window,
                cx,
                32,
                source_node_id,
                PresentationTarget::Browser {
                    url: "https://example.com/b".to_string(),
                },
            );
            let old_a = execute_present(
                app,
                window,
                cx,
                33,
                source_node_id,
                PresentationTarget::Browser {
                    url: "https://example.com/a".to_string(),
                },
            );
            (current_b, old_a)
        })
    });
    let reused_b_node_id = presented_node_id(&current_b.result, "导航后的 B");
    let new_a_node_id = presented_node_id(&old_a.result, "再次展示 A");
    let (browser_nodes, snapshot) = (
        read_runtime(&app, cx, |runtime| {
            runtime
                .document
                .nodes()
                .iter()
                .filter_map(|node| {
                    let CanvasNodeKind::Browser(spec) = &node.kind else {
                        return None;
                    };
                    Some((node.id, spec.url.clone()))
                })
                .collect::<Vec<_>>()
        }),
        app.read_with(cx, |app, _cx| app.presentation_e2e_snapshot()),
    );

    assert_presented(
        &current_b.result,
        PresentationKind::Browser,
        PresentationDisposition::Reused,
        "导航后的 B",
    );
    assert_presented(
        &old_a.result,
        PresentationKind::Browser,
        PresentationDisposition::Created,
        "再次展示 A",
    );
    assert_eq!(
        reused_b_node_id, first_node_id,
        "Present B 必须复用已经从 A 导航到 B 的原 Browser"
    );
    assert_ne!(
        new_a_node_id, first_node_id,
        "原 Browser 当前地址变成 B 后，再 Present A 必须创建新节点"
    );
    assert_eq!(
        browser_nodes.len(),
        2,
        "三次 Present 后必须只有当前 B 与新 A 两张 Browser"
    );
    assert!(
        browser_nodes
            .iter()
            .any(|(node_id, url)| *node_id == first_node_id && url == "https://example.com/b"),
        "原 Browser 节点必须持久化 native 导航后的 B 地址"
    );
    assert!(
        browser_nodes
            .iter()
            .any(|(node_id, url)| *node_id == new_a_node_id && url == "https://example.com/a"),
        "再次 Present A 必须保留一张独立的新 A Browser"
    );
    assert_eq!(
        snapshot.master_id,
        Some(new_a_node_id),
        "最后创建的新 A Browser 必须成为 Master"
    );
    assert_ne!(
        reused_b_node_id, new_a_node_id,
        "B 的复用回执不得错误返回新 A 的 node_id"
    );
}

fn create_source_agent(app: &Entity<MultisoulApp>, cx: &mut VisualTestContext) -> u64 {
    cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            let result = app
                .execute_agent_card_command_for_e2e(
                    window,
                    CardCommand::CreateAgent {
                        name: Some("Alan".to_string()),
                        profile: AgentProfile::Codex,
                        activate: true,
                    },
                )
                .expect("展示 e2e 应激活 Alan/Codex");
            let CardCommandResult::AgentCreated {
                node_id: Some(node_id),
                ..
            } = result
            else {
                panic!("Alan 激活必须返回 node_id");
            };
            node_id
        })
    })
}

fn execute_present(
    app: &mut MultisoulApp,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<MultisoulApp>,
    request_id: u64,
    source_node_id: u64,
    target: PresentationTarget,
) -> norma::protocol::WireResponse {
    app.execute_norma_wire_request_for_e2e(
        window,
        cx,
        WireRequest {
            protocol_version: norma::protocol::CURRENT_PROTOCOL_VERSION,
            client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            request_id,
            operation_id: None,
            authorization: Some(ProviderAuthorization {
                provider: AgentProfile::Codex,
                token: "codex-presentation-token".to_string(),
            }),
            source_workspace_id: None,
            source_workspace_route_id: None,
            target_workspace_id: None,
            source_node_id: Some(source_node_id),
            command: CardCommand::Present {
                target,
                create_new: false,
            },
        },
    )
}

fn assert_presented(
    result: &Result<CardCommandResult, norma::protocol::CardCommandError>,
    kind: PresentationKind,
    disposition: PresentationDisposition,
    label: &str,
) {
    assert!(
        matches!(
            result,
            Ok(CardCommandResult::Presented {
                kind: actual_kind,
                disposition: actual_disposition,
                layout: PresentationLayout::Master,
                ..
            }) if *actual_kind == kind && *actual_disposition == disposition
        ),
        "{label} 必须返回期望 kind/disposition 且确认 Master；实际：{result:?}"
    );
}

fn presented_node_id(
    result: &Result<CardCommandResult, norma::protocol::CardCommandError>,
    label: &str,
) -> u64 {
    let Ok(CardCommandResult::Presented { node_id, .. }) = result else {
        panic!("{label} 必须返回 Presented；实际：{result:?}");
    };
    *node_id
}
