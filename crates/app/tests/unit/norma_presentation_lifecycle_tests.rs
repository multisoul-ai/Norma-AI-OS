use super::*;
use norma::protocol::{
    AgentErrorCode, AgentProfile as NormaAgentProfile, CardCommandError, PresentationDisposition,
    PresentationKind, PresentationLayout, PresentationTarget, ProviderAuthorization, WireRequest,
};

/// Present Socket 生产路径：复用编排授权、刷新 Preview、切换 Master 并合并来源通知。
///
/// 数据构造（含关键数值的推导过程）：
///   来源 Agent = Alan/Codex，初始 Agent 节点数=1
///   有效请求数=2：Preview + Browser；非法请求数=1：错误 Token Browser
///   Preview 文件 bytes=8（"# Report"）
///   同来源通知窗口=5 秒；连续展示计数期望=2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开临时项目、登记 Codex Token 并激活 Alan。
///   2. 通过生产 execute_card_command 展示 Preview，再展示 Browser。
///   3. 立即读取 Preview Loading、Master、Canvas 焦点和合并通知。
///   4. 用错误 Token 再请求一个 Browser，确认鉴权先于副作用。
///
/// 预期结果：
///   - 正断言：两个有效请求均返回 Created/Master，最后 Browser 成为 Master。
///   - 正断言：Preview 已进入后台 Loading；通知来源为 Alan 且 count=2。
///   - 正断言：非法请求返回 Unauthorized。
///   - 负断言：非法请求不得创建第四个节点或改变通知计数。
#[test]
fn test_presentation_socket_authorizes_source_loads_preview_and_coalesces_notification() {
    let mut gpui = gpui::TestAppContext::single();
    let workspace = tempfile::TempDir::new().expect("Present Socket 测试应创建临时项目");
    let preview_path = workspace.path().join("report.md");
    std::fs::write(&preview_path, "# Report").expect("测试应写入 Preview 文件");
    let preview_path = std::fs::canonicalize(preview_path).expect("Preview 路径应可 canonicalize");
    let authorization_store =
        storage::OrchestrationAuthorizationStore::new(workspace.path().join("presentation.sqlite"));
    authorization_store
        .authorize("codex", "codex-token", "1.0.0")
        .expect("测试应登记 Codex Token");
    let authorization_cache = storage::OrchestrationAuthorizationCache::new(
        authorization_store
            .list()
            .expect("测试应加载 Codex 授权哈希"),
    );
    let (app, cx) = gpui.add_window_view(|_window, cx| {
        crate::app::MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_cmd, _cols, _rows, tx| {
                Ok(Box::new(terminal::TestPtyBackend::echo(tx)) as Box<dyn terminal::PtyBackend>)
            }),
        )
    });
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx);
        app.orchestration_authorizations = authorization_cache;
    });
    let source_node_id = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            let created = crate::app::execute_agent_card_command(
                app,
                window,
                CardCommand::CreateAgent {
                    name: Some("Alan".to_string()),
                    profile: NormaAgentProfile::Codex,
                    activate: true,
                },
            )
            .expect("测试应激活 Alan/Codex");
            let CardCommandResult::AgentCreated {
                node_id: Some(node_id),
                ..
            } = created
            else {
                panic!("激活 Alan 应返回 AgentCreated 与 node_id");
            };
            node_id
        })
    });

    let (preview, browser, unauthorized, snapshot) = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            let authorization = || ProviderAuthorization {
                provider: NormaAgentProfile::Codex,
                token: "codex-token".to_string(),
            };
            let preview = app.execute_card_command(
                WireRequest {
                    protocol_version: norma::protocol::CURRENT_PROTOCOL_VERSION,
                    client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
                    request_id: 11,
                    operation_id: Some("presentation-11".to_string()),
                    authorization: Some(authorization()),
                    source_workspace_id: None,
                    source_workspace_route_id: None,
                    target_workspace_id: None,
                    source_node_id: Some(source_node_id),
                    command: CardCommand::Present {
                        target: PresentationTarget::Preview {
                            path: preview_path,
                            explicit_type: None,
                        },
                        create_new: false,
                    },
                },
                Some(window),
                cx,
            );
            let preview_node_id = match &preview.result {
                Ok(CardCommandResult::Presented { node_id, .. }) => *node_id,
                ref other => panic!("有效 Preview Present 应成功；实际：{other:?}"),
            };
            let browser = app.execute_card_command(
                WireRequest {
                    protocol_version: norma::protocol::CURRENT_PROTOCOL_VERSION,
                    client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
                    request_id: 12,
                    operation_id: Some("presentation-12".to_string()),
                    authorization: Some(authorization()),
                    source_workspace_id: None,
                    source_workspace_route_id: None,
                    target_workspace_id: None,
                    source_node_id: Some(source_node_id),
                    command: CardCommand::Present {
                        target: PresentationTarget::Browser {
                            url: "https://example.com/review".to_string(),
                        },
                        create_new: false,
                    },
                },
                Some(window),
                cx,
            );
            let browser_node_id = match &browser.result {
                Ok(CardCommandResult::Presented { node_id, .. }) => *node_id,
                ref other => panic!("有效 Browser Present 应成功；实际：{other:?}"),
            };
            let unauthorized = app.execute_card_command(
                WireRequest {
                    protocol_version: norma::protocol::CURRENT_PROTOCOL_VERSION,
                    client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
                    request_id: 13,
                    operation_id: Some("presentation-13".to_string()),
                    authorization: Some(ProviderAuthorization {
                        provider: NormaAgentProfile::Codex,
                        token: "wrong-token".to_string(),
                    }),
                    source_workspace_id: None,
                    source_workspace_route_id: None,
                    target_workspace_id: None,
                    source_node_id: Some(source_node_id),
                    command: CardCommand::Present {
                        target: PresentationTarget::Browser {
                            url: "https://example.com/forbidden".to_string(),
                        },
                        create_new: false,
                    },
                },
                Some(window),
                cx,
            );
            let notification = app
                .presentation_notification
                .as_ref()
                .expect("两个有效 Present 后应立即显示来源通知");
            let snapshot = (
                preview_node_id,
                browser_node_id,
                app.previews.state_for(preview_node_id).cloned(),
                app.master_layout.master_id(),
                app.runtime.focus.mode(),
                notification.source_name.clone(),
                notification.count,
                app.runtime.document.nodes().len(),
            );
            (preview, browser, unauthorized, snapshot)
        })
    });

    assert!(
        matches!(
            preview.result,
            Ok(CardCommandResult::Presented {
                kind: PresentationKind::Preview,
                disposition: PresentationDisposition::Created,
                layout: PresentationLayout::Master,
                ..
            })
        ),
        "有效 Preview 请求必须返回 Created/Master；实际：{preview:?}"
    );
    assert!(
        matches!(
            browser.result,
            Ok(CardCommandResult::Presented {
                kind: PresentationKind::Browser,
                disposition: PresentationDisposition::Created,
                layout: PresentationLayout::Master,
                ..
            })
        ),
        "有效 Browser 请求必须返回 Created/Master；实际：{browser:?}"
    );
    assert!(
        matches!(snapshot.2, Some(crate::preview::PreviewLoadState::Loading)),
        "Present Preview 成功后必须立即启动后台读取并进入 Loading"
    );
    assert_eq!(
        snapshot.3,
        Some(snapshot.1),
        "最后展示的 Browser 必须成为 Master"
    );
    assert_eq!(
        snapshot.4,
        runtime::InputMode::CanvasSelect,
        "连续 Present 后焦点必须保持 CanvasSelect"
    );
    assert_eq!(
        snapshot.5, "Alan",
        "展示通知必须标注真实来源 Agent 名称 Alan"
    );
    assert_eq!(
        snapshot.6, 2,
        "同一来源五秒窗口内两次展示必须合并为 count=2"
    );
    assert_eq!(
        snapshot.7, 3,
        "Alan + Preview + Browser 共应恰有三个节点，非法请求不得创建第四个"
    );
    assert!(
        matches!(
            unauthorized.result,
            Err(CardCommandError::Agent(ref failure))
                if failure.code == AgentErrorCode::Unauthorized
        ),
        "错误 Token 的 Present 必须返回 Unauthorized；实际：{unauthorized:?}"
    );
    assert_ne!(
        snapshot.3,
        Some(snapshot.0),
        "第二次展示完成后 Preview 不得继续占用 Master"
    );
}
