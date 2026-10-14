use super::test_support::Fixture;
use norma::agent_hook::{forward_event, parse_hook_event, HookSource};
use serde_json::json;
use std::time::Duration;

fn hook_frame(f: &mut Fixture, node: u64, event: &str, prompt: Option<&str>) {
    let config = f.test.update(|cx| {
        f.sessions[0]
            .read(cx)
            .agent_hook_server
            .runtime_config()
            .expect("本机 Hook 入口")
    });
    let hook = parse_hook_event(
        HookSource::Codex,
        node,
        json!({"hook_event_name":event,"prompt":prompt}),
    )
    .expect("真实 Provider 事件解析");
    forward_event(&config.endpoint, &config.token, &hook).expect("真实 HTTP 投递 Hook");
    f.test.update(|cx| {
        assert!(
            f.sessions[0].read(cx).agent_hook_server.has_pending(),
            "事件实际进入生产 Hook 队列，未绕过 drain"
        )
    });
    f.test.run_until_parked();
    // 24ms = 2 × 12ms runtime 帧间隔；运行真实 frame_loop，而非手调登记函数。
    f.test
        .background_executor
        .advance_clock(Duration::from_millis(24));
    f.test.run_until_parked();
    f.test.update(|cx| {
        assert!(
            !f.sessions[0].read(cx).agent_hook_server.has_pending(),
            "生产 frame_loop 必须消费 Hook"
        )
    });
}
fn task_count(f: &mut Fixture) -> usize {
    // 等待帧产生的后台写入，避免 list_tasks 在 INSERT 之前读取而假通过。
    let writer = f.test.update(|cx| {
        f.sessions[0]
            .read(cx)
            .persistence
            .agent_dispatch_writer()
            .cloned()
            .expect("项目任务持久化队列")
    });
    writer.flush().expect("后台任务写入完成");
    f.call("list_tasks", json!({}))["tasks"]
        .as_array()
        .expect("任务列表")
        .len()
}

/// 场景：无活跃任务的卡片接收 Connector 回复，后续 Hook 不得登记外部根任务。
/// 构造：临时区域、echo PTY、真实 Hook HTTP；两条正文具有相同 60 字展示前缀。
/// 执行：reply → HTTP UserPromptSubmit → 生产 frame_loop → list_tasks；再发同前缀用户任务、迟到回复 Hook、相同正文用户任务。
/// 预期：Connector Hook 不增加任务，普通用户输入仍登记，来源标记精确匹配全文且只消费一次。
#[test]
fn connector_reply_hook_does_not_register_a_new_task() {
    let mut f = Fixture::new();
    let card = f.create("card", 0, None);
    let node = f
        .repo
        .signer
        .verify(card["card_id"].as_str().expect("卡片"))
        .expect("卡片身份")
        .node_id;
    let message = format!("{}A", "前".repeat(60));
    let human = format!("{}B", "前".repeat(60));
    assert_eq!(
        norma::agent_hook::extract_prompt_text(&json!({"prompt":message})),
        norma::agent_hook::extract_prompt_text(&json!({"prompt":human})),
        "两种正文展示标题相同"
    );
    let args = json!({"operation_id":"reply","card_id":card["card_id"],"card_revision":card["card_revision"],"message":message});
    hook_frame(&mut f, node, "SessionStart", None);
    assert_eq!(task_count(&mut f), 0, "初始无任务，避免活跃任务掩盖误建");
    assert_eq!(
        f.call("reply_agent", args.clone())["reply_status"],
        "sent",
        "回复实际提交 PTY"
    );
    // CLI 启动 Hook 可在回复 Enter 之后到达，重新变为 Ready 后提交 Hook 才产生状态迁移。
    hook_frame(&mut f, node, "SessionStart", None);
    hook_frame(&mut f, node, "UserPromptSubmit", Some(&message));
    assert_eq!(
        task_count(&mut f),
        0,
        "完整回复 Hook 帧之后 list_tasks 不增加"
    );
    hook_frame(&mut f, node, "Stop", None);
    let mut second = args;
    second["operation_id"] = json!("second-reply");
    assert_eq!(
        f.call("reply_agent", second)["reply_status"],
        "sent",
        "第二个回复已有来源标记"
    );
    hook_frame(&mut f, node, "SessionStart", None);
    hook_frame(&mut f, node, "UserPromptSubmit", Some(&human));
    assert_eq!(
        task_count(&mut f),
        1,
        "相同展示前缀的普通用户任务不能被吞掉"
    );
    hook_frame(&mut f, node, "Stop", None);
    hook_frame(&mut f, node, "UserPromptSubmit", Some(&message));
    assert_eq!(task_count(&mut f), 1, "迟到回复 Hook 仍不能登记第二个任务");
    hook_frame(&mut f, node, "Stop", None);
    hook_frame(&mut f, node, "UserPromptSubmit", Some(&message));
    assert_eq!(
        task_count(&mut f),
        2,
        "来源只消费一次，相同正文的用户任务仍正常登记"
    );
}

/// 场景：Connector 回复因审批等待而排队，释放输入后异步发送 Enter，其 Hook 仍不能建任务。
/// 构造：临时卡片、真实 PermissionRequest/SessionStart Hook，队列 Enter 延迟为 100ms。
/// 执行：reply 返回 queued，推进生产帧类型阶段、等待 120ms（100ms 加 20ms 裕量）后推进发送帧，再投递提交 Hook。
/// 预期：排队条目发送后移除，完整 Hook 链路 list_tasks 仍为零。
#[test]
fn connector_queued_reply_hook_preserves_origin_until_submission() {
    let mut f = Fixture::new();
    let card = f.create("queued-card", 0, None);
    let node = f
        .repo
        .signer
        .verify(card["card_id"].as_str().expect("卡片"))
        .expect("身份")
        .node_id;
    hook_frame(&mut f, node, "PermissionRequest", None);
    let message = "审批后的会话回复";
    let reply = f.call("reply_agent", json!({"operation_id":"queued-reply","card_id":card["card_id"],"card_revision":card["card_revision"],"message":message}));
    assert_eq!(reply["reply_status"], "queued", "等待审批时先排队");
    hook_frame(&mut f, node, "SessionStart", None);
    std::thread::sleep(Duration::from_millis(120));
    f.test
        .background_executor
        .advance_clock(Duration::from_millis(24));
    f.test.run_until_parked();
    f.test.update(|cx| {
        assert!(
            f.sessions[0]
                .read(cx)
                .session_reply_queue
                .peek_reply(
                    reply["reply_id"]
                        .as_str()
                        .expect("回复编号")
                        .parse()
                        .expect("数字编号")
                )
                .is_none(),
            "排队回复实际发送 Enter 后移除"
        )
    });
    hook_frame(&mut f, node, "SessionStart", None);
    hook_frame(&mut f, node, "UserPromptSubmit", Some(message));
    assert_eq!(
        task_count(&mut f),
        0,
        "排队来源延续到提交 Hook，不得变为根任务"
    );
}

/// 场景：已有用户任务处于 Thinking，回复提交 Hook 不迁移状态，仍须消费来源。
/// 构造：临时卡片、一条用户根任务、一次即时 Connector 回复；后续用户恰好使用同一回复正文。
/// 执行：用户提交 Hook → reply → 同状态提交 Hook → Stop → 同正文用户 Hook，均经真实 HTTP 和生产帧。
/// 预期：回复不增加任务，后续用户产生第二个根任务，不被旧来源标记吞掉。
#[test]
fn connector_reply_hook_consumes_origin_without_state_change() {
    let mut f = Fixture::new();
    let card = f.create("unchanged-card", 0, None);
    let node = f
        .repo
        .signer
        .verify(card["card_id"].as_str().expect("卡片"))
        .expect("身份")
        .node_id;
    hook_frame(&mut f, node, "SessionStart", None);
    hook_frame(&mut f, node, "UserPromptSubmit", Some("用户原有任务"));
    assert_eq!(task_count(&mut f), 1, "先登记用户根任务");
    let message = "下一步继续";
    let reply = f.call("reply_agent", json!({"operation_id":"unchanged-reply","card_id":card["card_id"],"card_revision":card["card_revision"],"message":message}));
    assert_eq!(reply["reply_status"], "sent", "即时回复已发送");
    f.test.update(|cx| {
        assert_eq!(
            f.sessions[0].read(cx).runtime.agent_task_state(node),
            Some(runtime::AgentTaskState::Thinking),
            "已有用户任务处于 Thinking，随后 Hook 不迁移状态"
        )
    });
    hook_frame(&mut f, node, "UserPromptSubmit", Some(message));
    assert_eq!(task_count(&mut f), 1, "同状态回复 Hook 不增加任务");
    hook_frame(&mut f, node, "Stop", None);
    hook_frame(&mut f, node, "UserPromptSubmit", Some(message));
    assert_eq!(
        task_count(&mut f),
        2,
        "同正文用户任务不能被未消费的旧来源吞掉"
    );
}
