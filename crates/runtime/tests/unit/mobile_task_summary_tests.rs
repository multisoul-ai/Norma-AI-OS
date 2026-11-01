use super::*;
use norma::agent_hook::{parse_hook_event, AgentHookEvent, HookSource};
use serde_json::{json, Value};

fn hook(
    node: NodeId,
    source: HookSource,
    name: &str,
    turn: &str,
    mut raw: Value,
) -> AgentHookEvent {
    raw["hook_event_name"] = json!(name);
    raw["session_id"] = json!("summary-session");
    raw["turn_id"] = json!(turn);
    parse_hook_event(source, node, raw).expect("明确的原卡事件")
}

fn excerpt(runtime: &Runtime, node: NodeId) -> Value {
    let card = runtime
        .mobile_card_snapshots()
        .into_iter()
        .find(|c| c.node_id == node)
        .expect("原卡快照");
    serde_json::to_value(card).expect("正式编码")["task_summary"].clone()
}

/// 场景：原生 Codex 终态替代 Stop 后，完成摘要仍须按真实结果冻结。
/// 构造：同一回合先有工具说明，再分别收到 completed/failed/interrupted。
/// 执行：提交 → 工具说明 → 原生终态，读取既有摘要接口。
/// 预期：完成取最终回复，失败取原错误，中断清空；不能遗留等待中的工具说明。
#[test]
fn native_codex_completion_keeps_the_correct_result_summary() {
    for (status, expected) in [
        ("completed", Some("build verified")),
        ("failed", Some("connection lost")),
        ("interrupted", None),
    ] {
        let mut summary = summary::TaskSummary::default();
        summary.observe_hook(&hook(
            1,
            HookSource::Codex,
            "UserPromptSubmit",
            "a",
            json!({}),
        ));
        summary.observe_hook(&hook(
            1,
            HookSource::Codex,
            "PreToolUse",
            "a",
            json!({"tool_input":{"description":"waiting for terminal"}}),
        ));
        summary.observe_hook(&hook(1, HookSource::Codex, "turn/completed", "a", json!({
            "turn":{"status":status}, "last_assistant_message":"build verified", "message":"connection lost"
        })));
        assert_eq!(
            summary.snapshot().as_deref(),
            expected,
            "{status} 必须保存对应终态摘要"
        );
        assert_ne!(
            summary.snapshot().as_deref(),
            Some("waiting for terminal"),
            "终态不得保留过程中的等待说明"
        );
    }
}

/// 原生两类卡片展示当前回合的操作说明与最终回复，新回合清空且旧事件不能污染。
/// 数据构造：2张原PTY卡使用同一合成session但node不同；每个回合有1次带description的工具及1次Stop。
///   240字符为快照摘录上限，后续边界测试单独覆盖；此处短句不截断。
/// 执行过程：启动两类卡 → 当前工具说明 → 最终回复与完成摘要 → 下一回合 → 旧回合Stop → 不含说明的工具。
/// 预期结果：正断言为真实说明/回复及冻结的完成摘要；命令正文不进入摘要，新回合和无说明工具不保留旧文；PTY数量始终为2。
#[test]
fn native_task_summary_is_scoped_to_current_turn() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let codex = runtime
        .spawn_agent(
            AgentProfile::Codex,
            CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .expect("Codex原卡");
    let claude = runtime
        .spawn_agent(
            AgentProfile::Claude,
            CanvasRect::new(700., 0., 640., 400.),
            80,
            24,
        )
        .expect("Claude原卡");
    for (node, source) in [(codex, HookSource::Codex), (claude, HookSource::Claude)] {
        assert!(
            runtime.observe_mobile_hook(&hook(
                node,
                source,
                "UserPromptSubmit",
                "a",
                json!({"prompt":"修复连接"})
            )),
            "当前回合已接受"
        );
        assert!(runtime.observe_mobile_hook(&hook(node, source, "PreToolUse", "a", json!({"tool_name":"Bash", "tool_input":{"description":"正在验证弱网恢复", "command":"SECRET_COMMAND_BODY"}}))), "工具事件已接受");
    }
    assert_eq!(
        excerpt(&runtime, codex),
        json!("正在验证弱网恢复"),
        "Codex应展示真实操作说明"
    );
    assert_eq!(
        excerpt(&runtime, claude),
        json!("正在验证弱网恢复"),
        "Claude应展示真实操作说明"
    );
    assert_ne!(
        excerpt(&runtime, codex),
        json!("SECRET_COMMAND_BODY"),
        "命令正文不能变成任务摘要"
    );
    assert!(
        runtime.observe_mobile_hook(&hook(
            codex,
            HookSource::Codex,
            "Stop",
            "a",
            json!({"last_assistant_message":"已修复重复转写，连接恢复测试通过。"})
        )),
        "准确Codex回复已接受"
    );
    assert_eq!(
        excerpt(&runtime, codex),
        json!("已修复重复转写，连接恢复测试通过。"),
        "最终回复应替换操作说明"
    );
    let (_, _, outcome) = runtime
        .native_mobile_event(&hook(
            codex,
            HookSource::Codex,
            "Stop",
            "a",
            json!({"last_assistant_message":"已修复重复转写，连接恢复测试通过。"}),
        ))
        .expect("准确的原生完成事件");
    assert_eq!(
        outcome.expect("完成证据").summary.as_deref(),
        Some("已修复重复转写，连接恢复测试通过。"),
        "通知摘要必须与该回合的完成证据一起冻结"
    );
    assert_eq!(
        excerpt(&runtime, claude),
        json!("正在验证弱网恢复"),
        "Codex回复不能污染另一张卡"
    );
    runtime.observe_mobile_hook(&hook(
        codex,
        HookSource::Codex,
        "UserPromptSubmit",
        "b",
        json!({}),
    ));
    assert_eq!(
        excerpt(&runtime, codex),
        Value::Null,
        "新回合不能展示上一轮成功回复"
    );
    assert!(
        !runtime.observe_mobile_hook(&hook(
            codex,
            HookSource::Codex,
            "Stop",
            "a",
            json!({"last_assistant_message":"旧回合重播"})
        )),
        "旧回合必须被入口拒绝"
    );
    assert_eq!(
        excerpt(&runtime, codex),
        Value::Null,
        "拒绝的旧事件不能恢复过期摘要"
    );
    runtime.observe_mobile_hook(&hook(
        claude,
        HookSource::Claude,
        "PreToolUse",
        "a",
        json!({"tool_name":"Read", "tool_input":{"file_path":"/private/file"}}),
    ));
    assert_eq!(
        excerpt(&runtime, claude),
        Value::Null,
        "新操作没有说明时不能继续展示上一操作"
    );
    assert_eq!(runtime.live_len(), 2, "摘要不能另开执行器或替换原PTY");
}

/// 摘录按Unicode标量裁剪，完整240字不截断，超长输出和空白处理保持有界。
/// 数据构造：240个“界”为上限，241=240+1应变为239字+省略号；4097个空格越过4096源标量扫描界限。
/// 执行过程：向当前回合发送不同agentMessage，再完成回合并重播迟到消息。
/// 预期结果：完整边界保留、越界明确省略、无控制字符或尾部泄漏，结束后不能被迟到回复覆盖。
#[test]
fn task_summary_bounds_and_completed_turn_are_stable() {
    let mut summary = summary::TaskSummary::default();
    summary.observe_codex("turn/started", &json!({"turn":{"id":"a"}}));
    let reply = |text: String| json!({"turnId":"a", "item":{"type":"agentMessage", "text":text}});
    summary.observe_codex("item/completed", &reply("界".repeat(240)));
    assert_eq!(
        summary.snapshot(),
        Some("界".repeat(240)),
        "恰好上限不能丢最后一个字"
    );
    summary.observe_codex("item/completed", &reply("界".repeat(241)));
    assert_eq!(
        summary.snapshot(),
        Some(format!("{}…", "界".repeat(239))),
        "越界保持240标量并明确省略"
    );
    summary.observe_codex(
        "item/completed",
        &reply("\n 已完成\t 👩‍💻\u{001b}  测试 \n".into()),
    );
    assert_eq!(
        summary.snapshot().as_deref(),
        Some("已完成 👩‍💻 测试"),
        "折叠空白、移除控制符但保留组合emoji"
    );
    summary.observe_codex(
        "item/completed",
        &reply(format!("{}扫描界限外", " ".repeat(4097))),
    );
    assert_eq!(summary.snapshot(), None, "不能扫描超过4096源标量");
    summary.observe_codex("item/completed", &reply("完成验证".into()));
    summary.observe_codex(
        "turn/completed",
        &json!({"turn":{"id":"a", "status":"completed"}}),
    );
    assert!(
        !summary.observe_codex("item/completed", &reply("迟到内容".into())),
        "已完成回合不能触发显示更新"
    );
    assert_eq!(
        summary.snapshot().as_deref(),
        Some("完成验证"),
        "最终摘录不能被迟到内容覆盖"
    );
}

/// 受控Codex仅正文变化也唤醒订阅，错线程/回合不污染，新回合撤销旧摘要。
/// 数据构造：现有本地WS fixture，当前thread-a/turn-a，负例为thread-other/turn-other；无真实Provider调用。
/// 执行过程：start → 仅回复事件 → 重复及错身份回复 → 完成 → 下一回合 → 失败。
/// 预期结果：正文变化使drain返回true，相同内容返回false；准确失败可见，原执行器数量不增加。
#[test]
fn controlled_task_summary_changes_wake_project_stream() {
    let (mut runtime, node, _directory, server) = codex::tests::setup();
    codex::tests::advance(
        &runtime,
        node,
        vec![
            json!({"method":"turn/started", "params":{"threadId":"thread-a","turn":{"id":"turn-a"}}}),
        ],
    );
    runtime.drain_all();
    let message = |thread: &str, turn: &str, text: &str| json!({"method":"item/completed", "params":{"threadId":thread,"turnId":turn,"item":{"type":"agentMessage","text":text}}});
    codex::tests::advance(
        &runtime,
        node,
        vec![message("thread-a", "turn-a", "正在核对连接路径")],
    );
    assert!(
        runtime.drain_mobile_codex(),
        "仅正文变化必须唤醒project订阅，不能等状态变化"
    );
    assert_eq!(
        excerpt(&runtime, node),
        json!("正在核对连接路径"),
        "正文必须投影到原卡"
    );
    codex::tests::advance(
        &runtime,
        node,
        vec![
            message("thread-a", "turn-a", "正在核对连接路径"),
            message("thread-other", "turn-a", "错误线程"),
            message("thread-a", "turn-other", "错误回合"),
        ],
    );
    assert!(!runtime.drain_mobile_codex(), "重复和错误身份不能制造更新");
    assert_eq!(
        excerpt(&runtime, node),
        json!("正在核对连接路径"),
        "拒绝的回复不能覆盖当前摘录"
    );
    codex::tests::advance(
        &runtime,
        node,
        vec![
            json!({"method":"turn/completed", "params":{"threadId":"thread-a","turn":{"id":"turn-a","status":"completed"}}}),
            json!({"method":"turn/started", "params":{"threadId":"thread-a","turn":{"id":"turn-b"}}}),
        ],
    );
    runtime.drain_mobile_codex();
    assert_eq!(
        excerpt(&runtime, node),
        Value::Null,
        "下一回合立即清空旧正文"
    );
    codex::tests::advance(
        &runtime,
        node,
        vec![
            json!({"method":"turn/completed", "params":{"threadId":"thread-a","turn":{"id":"turn-b","status":"failed","error":{"message":"连接中断"}}}}),
        ],
    );
    runtime.drain_mobile_codex();
    assert_eq!(
        excerpt(&runtime, node),
        json!("连接中断"),
        "失败回合只能显示真实错误"
    );
    assert_eq!(runtime.live_len(), 1, "不能创建第二执行器");
    runtime.kill_all();
    assert!(
        server.join().expect("回收隔离WS").is_empty(),
        "展示不能发送Provider命令"
    );
}
