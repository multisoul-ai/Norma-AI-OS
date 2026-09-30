//! Headless GPUI E2E：运行时直接发送会话消息，审批期间保留撤回保护。

use agent::{AdapterCommand, AdapterResult};
use canvas::AgentProfile;
use gpui::TestAppContext;
use norma::protocol::{AgentProfile as WireAgentProfile, CardCommand, CardCommandResult};
use runtime::{AgentTaskState, AgentTaskUpdate};
use std::time::Duration;

use crate::harness::{drain_until, enter_canvas, make_app, scripted_factory};

fn activate_alan(app: &gpui::Entity<app::MultisoulApp>, cx: &mut gpui::VisualTestContext) -> u64 {
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
    drain_until(
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

/// E2E-SESSION-REPLY-02：运行时 reply_agent 直接提交，不等当前任务结束。
///
/// 数据构造：
///   1 张 Alan 卡片、1 个 session、1 条消息；PTY 分别回显正文和 Enter 的标记。
///   覆盖 Thinking 与 RunningTool，两者均不先发送 Stop/Ready。
/// 执行过程：
///   1. 激活并绑定会话，设置运行状态后调用 reply_agent。
///   2. 保持运行状态，drain 验证正文与 Enter；再切到 Ready 并继续 drain。
/// 预期结果：
///   - 回执为 sent，运行中已提交且原 session 不变。
///   - Ready 后不重复发送，不新增独立 Agent task。
#[test]
fn busy_agent_sends_session_reply_without_waiting_or_resending() {
    assert_busy_reply_is_sent(AgentTaskState::Thinking);
    assert_busy_reply_is_sent(AgentTaskState::RunningTool {
        tool_name: Some("Bash".to_string()),
    });
}

fn assert_busy_reply_is_sent(state: AgentTaskState) {
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
                to: state.clone(),
                task_title: None,
                session_id: Some("codex-queue-session".to_string()),
            }),
            "运行状态应写入 Alan"
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
    } = result.expect("运行时应直接发送消息");
    assert_eq!(
        reply_status.as_deref(),
        Some("sent"),
        "运行时续聊应直接发送，实际 message={message}"
    );
    assert!(reply_id.is_some(), "发送回执应带 reply_id");
    assert_eq!(
        app.update(cx, |app, _| app.runtime.agent_task_state(node_id)),
        Some(state),
        "发送消息不得伪造当前任务已结束"
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
                (text.contains("QUEUED_TYPED") && text.contains("QUEUED_SUBMITTED")).then_some(text)
            }
            _ => None,
        },
        // 500ms / 12ms 每帧 ≈ 41 帧，足够消费同步测试 PTY 的正文与 Enter 回显。
        Duration::from_millis(500),
    )
    .expect("当前任务未结束前，CLI 应已收到正文和 Enter");

    assert!(
        after_text.contains("QUEUED_TYPED"),
        "PTY 应收到正文，grid：{after_text:?}"
    );
    assert!(
        after_text.contains("QUEUED_SUBMITTED"),
        "PTY 应收到 Enter，grid：{after_text:?}"
    );
    let task_count = app.update(cx, |app, _cx| app.agent_task_count_for_e2e());
    assert_eq!(task_count, 0, "续聊不得创建重复的 Agent task");
    assert_eq!(
        app.update(cx, |app, _| app.runtime.agent_session_id(node_id))
            .as_deref(),
        Some("codex-queue-session"),
        "消息必须进入原有会话"
    );
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
    let duplicate = drain_until(
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
                (text.matches("QUEUED_TYPED").count() > 1
                    || text.matches("QUEUED_SUBMITTED").count() > 1)
                    .then_some(text)
            }
            _ => None,
        },
        // 500ms = 5 × 旧队列的 100ms Enter 延迟，覆盖 Ready 后误补发窗口。
        Duration::from_millis(500),
    );
    assert!(
        duplicate.is_none(),
        "Ready 后不得重复投递，实际：{duplicate:?}"
    );
}

/// E2E-SESSION-REPLY-03：排队期间 cancel_agent_reply 撤回后，CLI 不得收到该文本。
///
/// 数据构造：WaitingApproval + reply queued；scripted 规则若收到正文会回 CANCEL_LEAKED。
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
                to: AgentTaskState::WaitingApproval {
                    tool_name: Some("Bash".to_string()),
                },
                task_title: None,
                session_id: Some("codex-cancel-session".to_string()),
            }),
            "WaitingApproval 应写入 Alan，禁止把正文和 Enter 注入审批界面"
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
