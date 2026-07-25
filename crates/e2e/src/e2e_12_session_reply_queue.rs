//! Headless GPUI E2E：忙碌会话回复排队、撤回，以及恢复后发送。

use agent::{AdapterCommand, AdapterResult};
use canvas::AgentProfile;
use gpui::TestAppContext;
use norma::protocol::{
    AgentProfile as WireAgentProfile, CardCommand, CardCommandResult,
};
use runtime::{AgentTaskState, AgentTaskUpdate};
use std::time::Duration;

use crate::harness::{drain_until, enter_canvas, make_app, scripted_factory};

fn activate_alan(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut gpui::VisualTestContext,
) -> u64 {
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
    match created.expect("应创建并激活 Alan") {
        CardCommandResult::AgentCreated {
            node_id: Some(node_id),
            ..
        } => node_id,
        other => panic!("激活创建应返回 node_id，实际：{other:?}"),
    }
}

fn bind_session(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut gpui::VisualTestContext,
    node_id: u64,
    session_id: &str,
) {
    let session_start = norma::agent_hook::parse_hook_event(
        norma::agent_hook::HookSource::Codex,
        node_id,
        serde_json::json!({
            "hook_event_name": "SessionStart",
            "session_id": session_id
        }),
    )
    .expect("SessionStart 可解析");
    app.update(cx, |app, _cx| {
        app.enqueue_agent_hook_event_for_e2e(session_start)
            .expect("enqueue SessionStart");
    });
    let _ = drain_until(
        app,
        cx,
        |runtime| {
            runtime
                .agent_session_id(node_id)
                .filter(|session| session == session_id)
                .map(|_| ())
        },
        Duration::from_millis(500),
    )
    .expect("SessionStart 后应缓存 session_id");
}

/// E2E-SESSION-REPLY-02：Thinking 时 reply_agent 返回 queued；回到 Ready 后只提交一次。
///
/// 数据构造：Alan 活跃 + session；runtime Thinking；scripted PTY 回执 QUEUED_TYPED/SUBMITTED。
/// 执行：reply → Queued；apply Ready；drain 直到 grid 出现回执。
/// 预期：首回执 queued；终态 PTY 含正文与 Enter；task_count=0。
#[test]
fn busy_agent_queues_session_reply_and_sends_once_when_ready() {
    let mut test_cx = TestAppContext::single();
    let rules = vec![
        (
            "补充要求：先写测试".as_bytes().to_vec(),
            b"QUEUED_TYPED\r\n".to_vec(),
        ),
        (b"\r".to_vec(), b"QUEUED_SUBMITTED\r\n".to_vec()),
    ];
    let (app, cx) = make_app(&mut test_cx, scripted_factory(rules));
    let workspace = tempfile::tempdir().expect("queue e2e workspace");
    enter_canvas(&app, cx, workspace.path());
    let node_id = activate_alan(&app, cx);
    bind_session(&app, cx, node_id, "codex-queue-session");

    app.update(cx, |app, _| {
        assert!(
            app.runtime.apply_agent_task_update(AgentTaskUpdate {
                node_id,
                profile: AgentProfile::Codex,
                provider_event: "UserPromptSubmit".to_string(),
                to: AgentTaskState::Thinking,
                task_title: None,
                session_id: Some("codex-queue-session".to_string()),
            }),
            "Thinking 应写入 Alan"
        );
    });

    let result = cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.execute_norma_adapter_command_for_e2e(
                window,
                cx,
                AdapterCommand::ReplyAgent {
                    agent: "Alan".to_string(),
                    message: "补充要求：先写测试".to_string(),
                },
            )
        })
    });
    let AdapterResult {
        reply_status,
        reply_id,
        message,
        ..
    } = result.expect("忙碌时应接受排队");
    assert_eq!(
        reply_status.as_deref(),
        Some("queued"),
        "忙碌续聊回执应为 queued，实际 message={message}"
    );
    assert!(reply_id.is_some(), "排队回执应带 reply_id");

    app.update(cx, |app, _| {
        assert!(
            app.runtime.apply_agent_task_update(AgentTaskUpdate {
                node_id,
                profile: AgentProfile::Codex,
                provider_event: "Stop".to_string(),
                to: AgentTaskState::Ready,
                task_title: None,
                session_id: Some("codex-queue-session".to_string()),
            }),
            "Ready 应解除忙碌"
        );
    });

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
                (text.contains("QUEUED_TYPED") && text.contains("QUEUED_SUBMITTED")).then_some(text)
            }
            _ => None,
        },
        Duration::from_millis(2_000),
    )
    .expect("就绪后应只提交一次排队回复");

    assert!(
        after_text.contains("QUEUED_TYPED"),
        "PTY 应收到排队正文，grid：{after_text:?}"
    );
    assert!(
        after_text.contains("QUEUED_SUBMITTED"),
        "PTY 应收到 Enter，grid：{after_text:?}"
    );
    let task_count = app.update(cx, |app, _cx| app.agent_task_count_for_e2e());
    assert_eq!(task_count, 0, "排队续聊不得创建 Agent task");
}

/// E2E-SESSION-REPLY-03：排队期间 cancel_agent_reply 撤回后，CLI 不得收到该文本。
///
/// 数据构造：Thinking + reply queued；scripted 规则若收到正文会回 CANCEL_LEAKED。
/// 执行：cancel_agent_reply(reply_id)；再 Ready 并 drain。
/// 预期：cancel 回执 cancelled；grid 不含 CANCEL_LEAKED。
#[test]
fn cancel_queued_session_reply_never_reaches_pty() {
    let mut test_cx = TestAppContext::single();
    let rules = vec![
        (
            "别发这条".as_bytes().to_vec(),
            b"CANCEL_LEAKED\r\n".to_vec(),
        ),
        (b"\r".to_vec(), b"CANCEL_ENTER\r\n".to_vec()),
    ];
    let (app, cx) = make_app(&mut test_cx, scripted_factory(rules));
    let workspace = tempfile::tempdir().expect("cancel e2e workspace");
    enter_canvas(&app, cx, workspace.path());
    let node_id = activate_alan(&app, cx);
    bind_session(&app, cx, node_id, "codex-cancel-session");

    app.update(cx, |app, _| {
        assert!(
            app.runtime.apply_agent_task_update(AgentTaskUpdate {
                node_id,
                profile: AgentProfile::Codex,
                provider_event: "UserPromptSubmit".to_string(),
                to: AgentTaskState::Thinking,
                task_title: None,
                session_id: Some("codex-cancel-session".to_string()),
            }),
            "Thinking 应写入 Alan"
        );
    });

    let queued = cx
        .update(|window, cx| {
            app.update(cx, |app, cx| {
                app.execute_norma_adapter_command_for_e2e(
                    window,
                    cx,
                    AdapterCommand::ReplyAgent {
                        agent: "Alan".to_string(),
                        message: "别发这条".to_string(),
                    },
                )
            })
        })
        .expect("应进入排队");
    let reply_id = queued.reply_id.expect("queued 应有 reply_id");
    assert_eq!(
        queued.reply_status.as_deref(),
        Some("queued"),
        "取消前应处于 queued"
    );

    let cancelled = cx
        .update(|window, cx| {
            app.update(cx, |app, cx| {
                app.execute_norma_adapter_command_for_e2e(
                    window,
                    cx,
                    AdapterCommand::CancelAgentReply {
                        reply_id: Some(reply_id),
                        agent: Some("Alan".to_string()),
                    },
                )
            })
        })
        .expect("应撤回排队回复");
    assert_eq!(
        cancelled.reply_status.as_deref(),
        Some("cancelled"),
        "撤回回执应为 cancelled，实际 message={}",
        cancelled.message
    );

    app.update(cx, |app, _| {
        assert!(
            app.runtime.apply_agent_task_update(AgentTaskUpdate {
                node_id,
                profile: AgentProfile::Codex,
                provider_event: "Stop".to_string(),
                to: AgentTaskState::Ready,
                task_title: None,
                session_id: Some("codex-cancel-session".to_string()),
            }),
            "Ready 后也不应补发已撤回回复"
        );
    });

    // 推进若干帧；若误发送，scripted PTY 会吐出 CANCEL_LEAKED。
    let leaked = drain_until(
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
                text.contains("CANCEL_LEAKED").then_some(text)
            }
            _ => None,
        },
        Duration::from_millis(800),
    );
    assert_eq!(
        leaked, None,
        "撤回后 CLI 不得收到正文，实际 grid：{leaked:?}"
    );
}
