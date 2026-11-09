use super::*;
use canvas::CanvasRect;
use norma::agent_hook::{parse_hook_event, HookSource};
use runtime::AgentHookRuntimeConfig;
use serde_json::{json, Value};

fn grok_runtime() -> (Runtime, u64, String) {
    let mut runtime = Runtime::new_for_workspace("/tmp/grok-pipeline");
    runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:49152/agent-hooks".into(),
        token: "test".into(),
        workspace_id: "w".into(),
        workspace_route_id: "area".into(),
        norma_socket: "/tmp/grok-pipeline.sock".into(),
    }));
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let node = runtime
        .spawn_agent(
            canvas::AgentProfile::Grok,
            CanvasRect::new(0.0, 0.0, 640.0, 400.0),
            80,
            24,
        )
        .expect("测试应能创建 Grok 卡");
    let run_id = runtime
        .mobile_agent_run_id(node)
        .expect("开了 hook 配置的 Grok 卡应有 control run")
        .to_owned();
    (runtime, node, run_id)
}

/// 按 Grok 真实 stdin 的形状造事件：camelCase 主键 + snake 别名，并带上 forward 注入的 run id。
fn grok_hook(
    node: u64,
    run_id: &str,
    name: &str,
    extra: Value,
) -> norma::agent_hook::AgentHookEvent {
    let mut raw = json!({
        "hook_event_name": name,
        "session_id": "sess-grok",
        "sessionId": "sess-grok",
        "_norma_control_run_id": run_id,
    });
    for (key, value) in extra.as_object().expect("extra 应是对象") {
        raw[key] = value.clone();
    }
    parse_hook_event(HookSource::Grok, node, raw).expect("Grok hook 应可解析")
}

/// 复现：Grok 卡片回合已 end_turn，卡片仍停在运行中（截图中 Bella 卡）。
///
/// 数据构造（取自 Grok 日志与抓到的真实 stdin，pid 27130 那条会话）：
///   SessionStart(source=new)                       t = 0
///   回合 1：UserPromptSubmit(promptId=p1, 问答测试)  t = 1_000
///          Stop(promptId=p1, reason=end_turn)      t = 33_000
///   回合 2：UserPromptSubmit(promptId=p2, "hi")      t = 100_000
///          Stop(promptId=p2, reason=end_turn)      t = 106_158（Worked for 6.2s）
///   每个 Stop 都在 +800ms（CONFIRMATION_WINDOW_MS）再 drain 一次。
///
/// 执行过程：
///   1. Stop 当帧：判定器发 KeepThinking → 卡片 Thinking（运行中）。
///   2. +800ms：判定器发 Confirm(Stop) → publish_mapped_event → Done。
///
/// 预期结果：
///   - 两个回合确认后卡片都是 Done，且各交出一条终态任务。
#[test]
fn grok_end_turn_stop_settles_card_through_real_publish_path() {
    let (mut runtime, node, run_id) = grok_runtime();
    let (sender, receiver) = crossbeam_channel::unbounded();
    let server = hook_server_for_test(receiver);
    let send = |name: &str, extra: Value| {
        sender
            .send(grok_hook(node, &run_id, name, extra))
            .expect("hook 应进入队列");
    };

    send("SessionStart", json!({"source": "new"}));
    server.drain_into_runtime_at(&mut runtime, None, 0);

    for (prompt_id, prompt, start, stop) in [
        ("p1", "测试一下问答工具", 1_000_u64, 33_000_u64),
        ("p2", "hi", 100_000, 106_158),
    ] {
        send(
            "UserPromptSubmit",
            json!({"prompt": prompt, "promptId": prompt_id}),
        );
        server.drain_into_runtime_at(&mut runtime, None, start);
        assert_eq!(
            runtime.agent_task_state(node),
            Some(AgentTaskState::Thinking),
            "{prompt_id}: 提交后卡片应是运行中"
        );
        send(
            "Stop",
            json!({
                "promptId": prompt_id, "reason": "end_turn",
                "stopHookActive": false, "stop_hook_active": false,
                "backgroundTasks": [], "sessionCrons": []
            }),
        );
        let held = server.drain_into_runtime_at(&mut runtime, None, stop);
        assert!(
            held.terminal_tasks.is_empty(),
            "{prompt_id}: Stop 当帧只开窗"
        );
        // 800 = CONFIRMATION_WINDOW_MS
        let done = server.drain_into_runtime_at(&mut runtime, None, stop + 800);
        assert_eq!(
            runtime.agent_task_state(node),
            Some(AgentTaskState::Done),
            "{prompt_id}: end_turn 的 Stop 确认后卡片必须是 Done，否则就是永久运行中"
        );
        assert_eq!(
            done.terminal_tasks.len(),
            1,
            "{prompt_id}: 确认后应恰好交出一条终态任务"
        );
    }
}

fn question_pre(node: u64, run_id: &str) -> norma::agent_hook::AgentHookEvent {
    grok_hook(
        node,
        run_id,
        "PreToolUse",
        json!({
            "tool_name": "ask_user_question", "tool_use_id": "call-q", "promptId": "p1",
            "tool_input": {"questions": [{"question": "Color?", "header": "Color",
                "options": [{"label": "Blue", "description": "b"}, {"label": "Green", "description": "g"}]}]}
        }),
    )
}

fn replay_question_session(native_first: bool, island_answer: bool) {
    let (mut runtime, node, run_id) = grok_runtime();
    runtime.set_native_first_questions(native_first);
    let (sender, receiver) = crossbeam_channel::unbounded();
    let server = hook_server_for_test(receiver);
    let send = |event| sender.send(event).expect("hook 应进入队列");

    send(grok_hook(
        node,
        &run_id,
        "SessionStart",
        json!({"source": "new"}),
    ));
    server.drain_into_runtime_at(&mut runtime, None, 0);
    send(grok_hook(
        node,
        &run_id,
        "UserPromptSubmit",
        json!({"prompt": "测试一下问答工具", "promptId": "p1"}),
    ));
    server.drain_into_runtime_at(&mut runtime, None, 1_000);

    // 同一 PreToolUse 走两组 hook：普通转发进判定器，--request-question 进岛。
    send(question_pre(node, &run_id));
    let ticket = norma::agent_hook::HookQuestion::new(
        norma::agent_hook::HookQuestionRequest {
            run_id: run_id.clone(),
            event: question_pre(node, &run_id),
        },
        std::time::Instant::now(),
    )
    .expect("问答 ticket");
    let target = runtime
        .accept_native_question(ticket.clone(), "project")
        .expect("岛应登记问答");
    server.drain_into_runtime_at(&mut runtime, None, 2_000);
    assert_eq!(
        runtime.agent_task_state(node),
        Some(AgentTaskState::WaitingQuestion),
        "问答登记后卡片应是等待回答"
    );

    if island_answer {
        let answers = vec![norma::protocol::QuestionAnswer {
            key: "question-0".into(),
            selected: vec!["option-0".into()],
            text: None,
        }];
        let submission = runtime
            .claim_question_answer(&target, answers, "delivery-grok")
            .expect("claim")
            .expect("submission");
        runtime
            .deliver_native_question(&submission)
            .expect("deliver");
        ticket
            .take_response(std::time::Instant::now())
            .expect("deny");
        ticket
            .with_response_write(std::time::Instant::now(), || Ok(true))
            .expect("write");
    }
    send(grok_hook(
        node,
        &run_id,
        "PostToolUse",
        json!({"tool_name": "ask_user_question", "tool_use_id": "call-q", "promptId": "p1",
            "tool_response": {"type": "AskUserQuestion", "UserAnswered": {"message": "Blue"}}}),
    ));
    server.drain_into_runtime_at(&mut runtime, None, 10_000);
    assert!(
        !runtime.has_pending_question(node),
        "作答后问答不能继续占着卡片"
    );

    for (prompt_id, stop) in [("p1", 33_000_u64), ("p2", 106_158)] {
        if prompt_id == "p2" {
            send(grok_hook(
                node,
                &run_id,
                "UserPromptSubmit",
                json!({"prompt": "hi", "promptId": "p2"}),
            ));
            server.drain_into_runtime_at(&mut runtime, None, 100_000);
        }
        send(grok_hook(
            node,
            &run_id,
            "Stop",
            json!({"promptId": prompt_id, "reason": "end_turn", "stopHookActive": false,
                "backgroundTasks": [], "sessionCrons": []}),
        ));
        server.drain_into_runtime_at(&mut runtime, None, stop);
        // 800 = CONFIRMATION_WINDOW_MS
        server.drain_into_runtime_at(&mut runtime, None, stop + 800);
        assert_eq!(
            runtime.agent_task_state(node),
            Some(AgentTaskState::Done),
            "{prompt_id}: 问答会话里 end_turn 的 Stop 确认后必须是 Done（native_first={native_first}, island={island_answer}）"
        );
    }
}

/// 回归：问答回合在终端作答后，本回合与下一回合的 Stop 都能把卡片收成 Done。
///
/// 数据构造：native_first=true（问答 hook 立即放回终端），PostToolUse 带 UserAnswered。
///   p1 Stop t=33_000，p2 Stop t=106_158，各在 +800ms（CONFIRMATION_WINDOW_MS）确认。
///
/// 执行过程：登记问答 → 卡片等待回答 → 终端作答 → 两个回合各自 Stop 并确认。
///
/// 预期结果：
///   - 作答后不再有待答问答。
///   - p1、p2 确认后卡片都是 Done，不停在运行中。
#[test]
fn grok_question_answered_in_terminal_then_turns_settle() {
    replay_question_session(true, false);
}

/// 回归：问答在灵动岛作答（hook 写回 deny）后，两个回合的 Stop 都能把卡片收成 Done。
///
/// 数据构造：native_first=false（问答 hook 留给岛），岛选 option-0 并写出 deny。
///   时间轴同终端作答用例。
///
/// 执行过程：登记问答 → 岛 claim + deliver + 写 stdout → PostToolUse → 两个回合 Stop 并确认。
///
/// 预期结果：
///   - 作答后不再有待答问答。
///   - p1、p2 确认后卡片都是 Done。
#[test]
fn grok_question_answered_on_island_then_turns_settle() {
    replay_question_session(false, true);
}

/// 复现：被门禁拒收的 Stop 仍经 KeepThinking 把卡片钉在运行中。
///
/// 数据构造：
///   SessionStart(sess-grok)                    t = 0
///   UserPromptSubmit(p1) → Stop 确认为 Done      t = 1_000 / 5_000 / 5_800
///   SessionStart(sess-new, source=clear)        t = 6_000（/clear 换会话，旧会话被 retire）
///   旧会话迟到的 Stop(p1, sess-grok)             t = 6_100
///     Grok 文档：回合结束报告在命令循环外派发，可晚于会话切换到达。
///   +800ms（CONFIRMATION_WINDOW_MS）再 drain    t = 6_900
///
/// 执行过程（修复前）：
///   1. 判定器 prompt_id 仍是 p1，迟到 Stop 不算 stale → on_stop 开窗 → KeepThinking。
///   2. publish_keep_thinking 不经 observe_mobile_hook，直接把 Ready 卡写成 Thinking。
///   3. +800ms Confirm → observe_mobile_hook 因 retired_sessions 拒收 → 静默 return。
///   4. 再无事件能把卡片拉回 → 永久运行中。
///
/// 预期结果：
///   - 迟到 Stop 前后卡片都不是 Thinking（门禁拒收的事件不能改卡片）。
///   - 不交出终态任务（旧会话的 Stop 不是新会话的完成）。
#[test]
fn rejected_late_stop_must_not_pin_card_to_running() {
    let (mut runtime, node, run_id) = grok_runtime();
    let (sender, receiver) = crossbeam_channel::unbounded();
    let server = hook_server_for_test(receiver);
    let send = |event| sender.send(event).expect("hook 应进入队列");
    let stop = json!({"promptId": "p1", "reason": "end_turn", "stopHookActive": false,
        "backgroundTasks": [], "sessionCrons": []});

    send(grok_hook(
        node,
        &run_id,
        "SessionStart",
        json!({"source": "new"}),
    ));
    server.drain_into_runtime_at(&mut runtime, None, 0);
    send(grok_hook(
        node,
        &run_id,
        "UserPromptSubmit",
        json!({"prompt": "hi", "promptId": "p1"}),
    ));
    server.drain_into_runtime_at(&mut runtime, None, 1_000);
    send(grok_hook(node, &run_id, "Stop", stop.clone()));
    server.drain_into_runtime_at(&mut runtime, None, 5_000);
    // 5_800 = 5_000 + CONFIRMATION_WINDOW_MS(800)
    server.drain_into_runtime_at(&mut runtime, None, 5_800);
    assert_eq!(
        runtime.agent_task_state(node),
        Some(AgentTaskState::Done),
        "前置：第一回合应正常完成"
    );

    let mut clear = grok_hook(node, &run_id, "SessionStart", json!({"source": "clear"}));
    clear.session_id = Some("sess-new".into());
    clear.raw_json["session_id"] = json!("sess-new");
    clear.raw_json["sessionId"] = json!("sess-new");
    send(clear);
    server.drain_into_runtime_at(&mut runtime, None, 6_000);
    let before = runtime.agent_task_state(node);
    assert_ne!(
        before,
        Some(AgentTaskState::Thinking),
        "前置：/clear 后卡片不在运行中"
    );

    send(grok_hook(node, &run_id, "Stop", stop));
    server.drain_into_runtime_at(&mut runtime, None, 6_100);
    assert_ne!(
        runtime.agent_task_state(node),
        Some(AgentTaskState::Thinking),
        "旧会话迟到的 Stop 被门禁拒收，KeepThinking 不能绕过门禁把卡片改成运行中"
    );
    // 6_900 = 6_100 + CONFIRMATION_WINDOW_MS(800)
    let settled = server.drain_into_runtime_at(&mut runtime, None, 6_900);
    assert_eq!(
        runtime.agent_task_state(node),
        before,
        "窗口结束后卡片应保持 /clear 后的状态，不能被钉在运行中"
    );
    assert!(
        settled.terminal_tasks.is_empty(),
        "旧会话的 Stop 不得交出新会话的终态任务"
    );
}
