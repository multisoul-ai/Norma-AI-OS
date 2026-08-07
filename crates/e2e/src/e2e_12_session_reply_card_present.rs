//! Headless GPUI E2E：画布上已有 Agent 卡片时，即使尚无 Hook session，也可 reply 对话。

use agent::{AdapterCommand, AdapterResult};
use gpui::TestAppContext;
use norma::protocol::{AgentProfile as WireAgentProfile, CardCommand, CardCommandResult};
use std::time::Duration;

use crate::harness::{drain_until, enter_canvas, make_app, scripted_factory};

/// E2E-SESSION-REPLY-CARD-01：有绑定 definition 的卡、无 SessionStart 时，reply_agent 仍应写入 PTY。
///
/// 数据构造：CreateAgent(Alan, activate=true)；不注入 SessionStart；scripted 回执 CARD_TYPED/SUBMITTED。
/// 执行：reply_agent("检查 README") → drain。
/// 预期：reply_status=sent；grid 含正文与 Enter；不得要求先激活/新建。
#[test]
fn reply_agent_works_when_card_exists_without_hook_session() {
    let mut test_cx = TestAppContext::single();
    let rules = vec![
        (
            "检查 README".as_bytes().to_vec(),
            b"CARD_TYPED\r\n".to_vec(),
        ),
        (b"\r".to_vec(), b"CARD_SUBMITTED\r\n".to_vec()),
    ];
    let (app, cx) = make_app(&mut test_cx, scripted_factory(rules));
    let workspace = tempfile::tempdir().expect("card-present e2e workspace");
    enter_canvas(&app, cx, workspace.path());

    let created = cx.update(|window, cx| {
        app.update(cx, |app, _cx| {
            app.execute_agent_card_command_for_e2e(
                window,
                CardCommand::CreateAgent {
                    name: Some("Alan".to_string()),
                    profile: WireAgentProfile::Codex,
                    activate: true,
                },
            )
        })
    });
    let node_id = match created.expect("应创建 Alan 卡片") {
        CardCommandResult::AgentCreated {
            node_id: Some(node_id),
            ..
        } => node_id,
        other => panic!("激活创建应返回 node_id，实际：{other:?}"),
    };

    let session = app.update(cx, |app, _| app.runtime.agent_session_id(node_id));
    assert_eq!(session, None, "本用例刻意不注入 SessionStart");

    let result = cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.execute_norma_adapter_command_for_e2e(
                window,
                cx,
                AdapterCommand::ReplyAgent {
                    agent: "Alan".to_string(),
                    message: "检查 README".to_string(),
                },
            )
        })
    });
    let AdapterResult {
        reply_status,
        message,
        ..
    } = result.expect("有卡时 reply_agent 应成功");
    assert_eq!(
        reply_status.as_deref(),
        Some("sent"),
        "有卡无 session 也应 sent，实际 message={message}"
    );
    assert!(
        !message.contains("激活") && !message.contains("创建"),
        "回执不得引导激活或新建，实际：{message}"
    );

    let after_text = drain_until(
        &app,
        cx,
        |runtime| match runtime.node_view(node_id) {
            Some(runtime::NodeView::Term { snapshot, .. }) => {
                let text = snapshot
                    .rows
                    .iter()
                    .flatten()
                    .map(|cell| cell.c)
                    .collect::<String>();
                (text.contains("CARD_TYPED") && text.contains("CARD_SUBMITTED")).then_some(text)
            }
            _ => None,
        },
        Duration::from_millis(1_000),
    )
    .expect("有卡即可对话，PTY 应收到正文与 Enter");

    assert!(
        after_text.contains("CARD_TYPED"),
        "同一张卡应收到对话正文，grid：{after_text:?}"
    );
    assert!(
        after_text.contains("CARD_SUBMITTED"),
        "同一张卡应收到 Enter，grid：{after_text:?}"
    );
}
