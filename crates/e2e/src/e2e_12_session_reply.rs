//! Headless GPUI E2E：Norma 会话续聊进入同一 Agent session，且不创建新任务。

use agent::{AdapterCommand, AdapterResult};
use gpui::TestAppContext;
use norma::protocol::{AgentProfile as WireAgentProfile, CardCommand, CardCommandResult};
use std::time::Duration;

use crate::harness::{drain_until, enter_canvas, make_app, read_runtime, scripted_factory};

/// E2E-SESSION-REPLY-01：注入 Alan 完整 A/B 回复后，Norma reply_agent 写入同一 session 并提交 Enter，
/// 且不新增 Agent task。
///
/// 数据构造：
///   CreateAgent(Alan/Codex, activate=true) → 1 张活跃卡片
///   Hook：SessionStart + UserPromptSubmit + Stop(final_response=A/B) → 1 条外部根任务
///   reply_agent(message="按照 A 方案执行")
///   scripted PTY：正文 → REPLY_TYPED；`\r` → REPLY_SUBMITTED
///
/// 执行过程：
///   1. 打开项目并创建激活 Alan
///   2. 注入用户消息与完整 Stop 回复，推进 frame loop → coordinator 记录 1 条外部根任务
///   3. 执行 reply_agent → 只向同一 session 写入，不进入任务派发
///   4. drain 直到 grid 出现 REPLY_TYPED 与 REPLY_SUBMITTED
///
/// 预期结果：
///   - 正断言：reply_status=sent；grid 含写入与回车回执；原有外部根任务数为 1
///   - 负断言：reply 前后任务数不得从 1 增至 2；不得把 Queued 说成唯一终态
#[test]
fn norma_reply_agent_continuation_reaches_same_pty_session_without_new_task() {
    let mut test_cx = TestAppContext::single();
    let rules = vec![
        (
            "按照 A 方案执行".as_bytes().to_vec(),
            b"REPLY_TYPED\r\n".to_vec(),
        ),
        (b"\r".to_vec(), b"REPLY_SUBMITTED\r\n".to_vec()),
    ];
    let (app, cx) = make_app(&mut test_cx, scripted_factory(rules));
    let workspace = tempfile::tempdir().expect("session reply e2e workspace");
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
    let created = created.expect("应创建并激活 Alan");
    let node_id = match &created {
        CardCommandResult::AgentCreated {
            node_id: Some(node_id),
            ..
        } => *node_id,
        other => panic!("激活创建应返回 node_id，实际：{other:?}"),
    };

    let session_start = norma::agent_hook::parse_hook_event(
        norma::agent_hook::HookSource::Codex,
        node_id,
        serde_json::json!({
            "hook_event_name": "SessionStart",
            "session_id": "codex-reply-session"
        }),
    )
    .expect("SessionStart 可解析");
    let user_prompt = norma::agent_hook::parse_hook_event(
        norma::agent_hook::HookSource::Codex,
        node_id,
        serde_json::json!({
            "hook_event_name": "UserPromptSubmit",
            "session_id": "codex-reply-session",
            "prompt": "给出 A/B 两个方案"
        }),
    )
    .expect("UserPromptSubmit 可解析");
    let stop = norma::agent_hook::parse_hook_event(
        norma::agent_hook::HookSource::Codex,
        node_id,
        serde_json::json!({
            "hook_event_name": "Stop",
            "session_id": "codex-reply-session",
            "final_response": "方案 A：快速上线；方案 B：先补测试。"
        }),
    )
    .expect("Stop 可解析");

    app.update(cx, |app, _cx| {
        app.enqueue_agent_hook_event_for_e2e(session_start)
            .expect("enqueue SessionStart");
        app.enqueue_agent_hook_event_for_e2e(user_prompt)
            .expect("enqueue UserPromptSubmit");
        app.enqueue_agent_hook_event_for_e2e(stop)
            .expect("enqueue Stop");
    });

    // Drain hook events into runtime/tails.
    let _ = drain_until(
        &app,
        cx,
        |runtime| {
            runtime
                .agent_session_id(node_id)
                .filter(|session| session == "codex-reply-session")
                .map(|_| ())
        },
        Duration::from_millis(500),
    )
    .expect("SessionStart 后应缓存 session_id");

    let task_count_before_reply = app.update(cx, |app, _cx| app.agent_task_count_for_e2e());
    assert_eq!(
        task_count_before_reply, 1,
        "UserPromptSubmit 应登记唯一一条外部根任务，实际 tasks={task_count_before_reply}"
    );

    let result = cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.execute_norma_adapter_command_for_e2e(
                window,
                cx,
                AdapterCommand::ReplyAgent {
                    agent: "Alan".to_string(),
                    message: "按照 A 方案执行".to_string(),
                },
            )
        })
    });
    let AdapterResult {
        reply_status,
        message,
        ..
    } = result.expect("reply_agent 应成功");
    assert_eq!(
        reply_status.as_deref(),
        Some("sent"),
        "就绪会话续聊回执应为 sent，实际 message={message}"
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
                (text.contains("REPLY_TYPED") && text.contains("REPLY_SUBMITTED")).then_some(text)
            }
            _ => None,
        },
        Duration::from_millis(1_000),
    )
    .expect("续聊应写入文本并提交 Enter");

    assert!(
        after_text.contains("REPLY_TYPED"),
        "同一 PTY 应收到续聊正文，grid：{after_text:?}"
    );
    assert!(
        after_text.contains("REPLY_SUBMITTED"),
        "同一 PTY 应收到 Enter，grid：{after_text:?}"
    );

    let task_count_after_reply = app.update(cx, |app, _cx| app.agent_task_count_for_e2e());
    assert_eq!(
        task_count_after_reply, task_count_before_reply,
        "会话续聊不得新增第二条 Agent task，reply 前={task_count_before_reply}，reply 后={task_count_after_reply}"
    );
    assert_eq!(
        task_count_after_reply, 1,
        "会话续聊后应只保留原有外部根任务，实际 tasks={task_count_after_reply}"
    );

    let session = read_runtime(&app, cx, |runtime| runtime.agent_session_id(node_id));
    assert_eq!(
        session.as_deref(),
        Some("codex-reply-session"),
        "续聊后仍应停留在同一 session"
    );
}
