use super::*;
use crate::{AgentTaskState, NodeInput};
use serde_json::json;
use std::time::Duration;

fn claude() -> (Runtime, NodeId, tempfile::TempDir) {
    claude_with_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }))
}

fn claude_with_factory(factory: crate::PtyFactory) -> (Runtime, NodeId, tempfile::TempDir) {
    let (mut runtime, node) = launch_tests::runtime_with_card();
    let directory = tempfile::tempdir().expect("隔离审计库");
    let launch = runtime
        .mobile_agent_launch(node, Path::new("unused"), 80, 24)
        .expect("标准80×24终端");
    let live = TermLive::spawn_with_factory(
        launch.generation,
        launch.command("session-a", "run-a"),
        80,
        24,
        &factory,
    )
    .expect("echo终端");
    runtime
        .attach_mobile_live(launch, "session-a".into(), "run-a".into(), live)
        .expect("附着");
    runtime.mobile_runs.get_mut(&node).expect("run").claude = Some(
        claude::ClaudeApprovals::new(storage::ApprovalAuditStore::new(
            directory.path().join("audit.db"),
        ))
        .expect("审计"),
    );
    runtime
        .events
        .update_agent_task_state(node, AgentTaskState::Ready, "test".into());
    (runtime, node, directory)
}

/// Claude Ready Hook 先到时，多行和 Tab 必须等终端声明安全粘贴后才写入。
///
/// 数据构造：80×24 recording PTY 初始未启用 bracketed paste；正文 first\n\tsecond。
///   包装头尾各6 bytes；正文5+1+1+6=13 bytes，完整粘贴共25 bytes；随后Enter为1 byte。
/// 执行过程：尝试LF和Tab → 检查零写入 → 开启安全粘贴 → Paste → 100ms仍未绘制 → 绘制后101ms → 准确Hook。
/// 预期结果：未准备时明确拒绝且不污染草稿；粘贴和Enter各一次；仅允许Claude原生Tab→4空格转换后确认。
#[test]
fn claude_multiline_message_waits_for_bracketed_paste_before_any_write() {
    use std::sync::{Arc, Mutex};
    let writes = Arc::new(Mutex::new(Vec::new()));
    let output = Arc::new(Mutex::new(None));
    let recorded = writes.clone();
    let sender = output.clone();
    let factory: crate::PtyFactory = Box::new(move |_, _, _, tx| {
        *sender.lock().expect("输出通道锁") = Some(tx.clone());
        Ok(Box::new(terminal::TestPtyBackend::recording(
            tx,
            recorded.clone(),
        )))
    });
    let (mut runtime, node, _directory) = claude_with_factory(factory);
    let expected = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        runtime
            .send_mobile_message(&expected, "first\nsecond".into())
            .err()
            .as_deref(),
        Some("terminal_not_ready"),
        "LF不能退化成裸Enter"
    );
    assert_eq!(
        runtime
            .send_mobile_message(&expected, "first\tsecond".into())
            .err()
            .as_deref(),
        Some("terminal_not_ready"),
        "Tab不能被CLI当成补全"
    );
    assert!(
        writes.lock().expect("记录锁").is_empty(),
        "拒绝必须发生在任何PTY写入之前"
    );
    assert!(
        runtime
            .validate_mobile_message(&expected, "single line")
            .is_ok(),
        "尚未写入不能污染草稿追踪"
    );
    output
        .lock()
        .expect("输出锁")
        .as_ref()
        .expect("输出通道")
        .send(terminal::PtyEvent::Output(b"\x1b[?2004h".to_vec()))
        .expect("启用安全粘贴");
    runtime.live.get_mut(&node).expect("终端").drain();
    let text = "first\n\tsecond";
    let receipt = runtime
        .send_mobile_message(&expected, text.into())
        .expect("准备好后可以原文发送");
    assert_eq!(
        *writes.lock().expect("记录锁"),
        vec![b"\x1b[200~first\n\tsecond\x1b[201~".to_vec()],
        "整段原文一次写入，不能带提前提交的CR"
    );
    assert!(receipt.try_recv().is_err(), "粘贴成功不能当成Provider确认");
    let pasted = Instant::now();
    runtime.advance_mobile_messages(pasted + Duration::from_millis(100));
    assert_eq!(
        writes.lock().expect("记录锁").len(),
        1,
        "Claude尚未绘制草稿时不能提前发Enter，否则会丢掉首次提交"
    );
    output
        .lock()
        .expect("输出锁")
        .as_ref()
        .expect("输出通道")
        .send(terminal::PtyEvent::Output(b"first\r\n    second".to_vec()))
        .expect("实际草稿绘制");
    runtime.live.get_mut(&node).expect("终端").drain();
    runtime.advance_mobile_messages(pasted + Duration::from_millis(101));
    // First paint at 101ms + 100ms native Enter guard = 201ms; 202ms crosses it.
    runtime.advance_mobile_messages(pasted + Duration::from_millis(202));
    assert_eq!(
        *writes.lock().expect("记录锁"),
        vec![
            b"\x1b[200~first\n\tsecond\x1b[201~".to_vec(),
            b"\r".to_vec()
        ],
        "草稿绘制后只提交一次Enter"
    );
    runtime.observe_mobile_hook(&prompt(node, "multiline-turn", "first\n    second"));
    assert_eq!(
        receipt.try_recv().expect("回执").expect("准确正文确认")["turn_id"],
        "multiline-turn",
        "多行原文必须绑定真实回合"
    );
    runtime.kill_all();
}

fn prompt(node: NodeId, turn: &str, text: &str) -> norma::agent_hook::AgentHookEvent {
    let mut event = norma::agent_hook::parse_hook_event(norma::agent_hook::HookSource::Claude, node,
        json!({"hook_event_name":"UserPromptSubmit","session_id":"session-a","prompt_id":turn,"prompt":text})).expect("Hook");
    event.control_run_id = Some("run-a".into());
    event
}

/// Claude 手机发送须等待准确 PromptSubmit；发送中的电脑输入和重复派发不能混入原消息。
///
/// 数据构造：run-a/session-a、echo终端、原文“手机任务”；100ms 为既有 paste / Enter 间隔。
/// 执行过程：发送 → 立即查回执 → 尝试另一输入 → 推进100ms → 送准确Hook → 查回执 → 关闭。
/// 预期结果：仅PTY写成功没有回执；输入和重发拒绝；Provider确认后返回准确turn，关闭后不能再发。
#[test]
fn claude_message_receipt_requires_exact_provider_acceptance() {
    let (mut runtime, node, _directory) = claude();
    let expected = runtime.mobile_card_snapshots().remove(0);
    let receipt = runtime
        .send_mobile_message(&expected, "手机任务".into())
        .expect("合法发送");
    assert!(receipt.try_recv().is_err(), "粘贴不等于Provider收到");
    assert!(
        runtime
            .send_mobile_message(&expected, "第二条".into())
            .is_err(),
        "同一时刻不能覆盖原提交"
    );
    assert!(
        runtime
            .send_node_input(node, NodeInput::Text("本地输入".into()))
            .is_err(),
        "两端输入不能混合"
    );
    runtime.live.get_mut(&node).expect("echo绘制").drain();
    let painted = Instant::now();
    runtime.advance_mobile_messages(painted);
    runtime.advance_mobile_messages(painted + Duration::from_millis(101));
    assert!(receipt.try_recv().is_err(), "Enter成功仍不是回执");
    assert!(
        runtime.observe_mobile_hook(&prompt(node, "turn-a", "手机任务")),
        "准确Hook接收"
    );
    let result = receipt.try_recv().expect("已有结果").expect("Provider确认");
    assert_eq!(result["node_id"], node, "准确卡片");
    assert_eq!(result["turn_id"], "turn-a", "准确回合");
    assert_eq!(result["run_id"], "run-a", "不能丢失运行身份");
    assert_eq!(result["session_id"], "session-a", "不能串到另一对话");
    assert!(receipt.try_recv().is_err(), "回执只能完成一次");
    runtime.kill_all();
    assert!(
        runtime
            .send_mobile_message(&expected, "迟到".into())
            .is_err(),
        "关闭后不得再发"
    );
}

/// 电脑草稿不能被手机覆盖；Hook不能清除其后新输入，错正文不能产生成功回执。
///
/// 数据构造：电脑草稿“本地”、Enter 后又输入“下一条”；1次手机发送等待到30秒期限之后。
/// 执行过程：写草稿 → 手机拒绝 → Enter/新输入/迟到Hook → 仍拒绝；新运行发送 → 错正文Hook；再新运行超时。
/// 预期结果：本地新草稿保留；不匹配或超时均结果未知，不允许假成功和自动重试。
#[test]
fn claude_drafts_mismatched_hook_and_timeout_do_not_fake_send() {
    let (mut runtime, node, _directory) = claude();
    runtime
        .send_node_input(node, NodeInput::Text("本地".into()))
        .expect("本地草稿");
    let expected = runtime.mobile_card_snapshots().remove(0);
    assert!(
        runtime
            .send_mobile_message(&expected, "手机".into())
            .is_err(),
        "不能粘贴到电脑草稿"
    );
    runtime
        .send_node_input(
            node,
            NodeInput::Key(terminal::KeyInput {
                key: terminal::Key::Enter,
                ctrl: false,
                alt: false,
                shift: false,
            }),
        )
        .expect("本地Enter");
    runtime
        .send_node_input(node, NodeInput::Text("下一条".into()))
        .expect("下一条草稿");
    runtime.observe_mobile_hook(&prompt(node, "turn-a", "本地"));
    assert!(
        runtime
            .send_mobile_message(&expected, "手机".into())
            .is_err(),
        "迟到Hook不得清除后续草稿"
    );
    runtime.kill_all();
    let (mut runtime, node, _other) = claude();
    let expected = runtime.mobile_card_snapshots().remove(0);
    let receipt = runtime
        .send_mobile_message(&expected, "原文".into())
        .expect("新提交");
    runtime.live.get_mut(&node).expect("echo绘制").drain();
    let painted = Instant::now();
    runtime.advance_mobile_messages(painted);
    runtime.advance_mobile_messages(painted + Duration::from_millis(101));
    runtime.observe_mobile_hook(&prompt(node, "turn-b", "并非原文"));
    assert!(
        receipt.try_recv().expect("明确结果").is_err(),
        "错正文不能算成功"
    );
    runtime.kill_all();
    let (mut runtime, _node, _last) = claude();
    let receipt = runtime
        .send_mobile_message(&runtime.mobile_card_snapshots().remove(0), "未确认".into())
        .expect("提交");
    // 31s = 30s发送确认上限 + 1s确定越过边界，无真实等待。
    runtime.advance_mobile_messages(Instant::now() + Duration::from_secs(31));
    assert!(
        receipt.try_recv().expect("超时结果").is_err(),
        "超时不得保持永久pending或假成功"
    );
    assert!(
        runtime
            .send_mobile_message(&runtime.mobile_card_snapshots().remove(0), "重试".into())
            .is_err(),
        "未知的终端草稿不能再次被覆盖"
    );
    runtime.kill_all();
}

/// Claude结束保留准确身份；新回合或尚未确认的桌面Enter不能冻结旧任务。
///
/// 数据构造：同一run-a/session-a的turn-a、turn-b；80×24 echo终端，无真实模型。
/// 执行过程：首轮提交/Stop → 比较准确身份 → 桌面Enter → 新回合提交/Stop → 关闭。
/// 预期结果：静止完成只允许本轮；活动、未确认Enter、旧turn及关闭后均拒绝捕获。
#[test]
fn delivery_requires_exact_quiescent_claude_turn() {
    let (mut runtime, node, _directory) = claude();
    runtime.observe_mobile_hook(&prompt(node, "turn-a", "first"));
    let key = runtime.mobile_latest_turn_key(node).expect("真实首轮身份");
    assert!(
        !runtime.mobile_delivery_can_capture(&key),
        "运行中不得冻结交付"
    );
    let stop = |turn| {
        let mut event = norma::agent_hook::parse_hook_event(
            norma::agent_hook::HookSource::Claude,
            node,
            json!({"hook_event_name":"Stop","session_id":"session-a","prompt_id":turn}),
        )
        .expect("Stop事件");
        event.control_run_id = Some("run-a".into());
        event
    };
    runtime.observe_mobile_hook(&stop("turn-a"));
    runtime
        .events
        .update_agent_task_state(node, AgentTaskState::Done, "Stop".into());
    assert!(
        runtime.mobile_delivery_can_capture(&key),
        "准确完成且无新提交时可捕获"
    );
    assert_eq!(
        runtime.mobile_latest_turn_key(node),
        Some(key.clone()),
        "完成不能丢失实际回合身份"
    );
    runtime
        .mobile_runs
        .get_mut(&node)
        .expect("run")
        .native_input
        .observe(
            &NodeInput::Key(terminal::KeyInput {
                key: terminal::Key::Enter,
                ctrl: false,
                alt: false,
                shift: false,
            }),
            false,
        );
    assert!(
        !runtime.mobile_delivery_can_capture(&key),
        "Enter已经提交，不能将Provider尚未通知理解成旧任务静止"
    );
    runtime.observe_mobile_hook(&prompt(node, "turn-b", "second"));
    runtime.observe_mobile_hook(&stop("turn-b"));
    runtime
        .events
        .update_agent_task_state(node, AgentTaskState::Done, "Stop".into());
    let second = runtime.mobile_latest_turn_key(node).expect("第二轮身份");
    assert_eq!(second.turn_id, "turn-b", "必须保留最新真实回合");
    assert!(
        runtime.mobile_delivery_can_capture(&second),
        "第二轮已结束可单独捕获"
    );
    assert!(
        !runtime.mobile_delivery_can_capture(&key),
        "相同session的旧回合不得借新Done复活"
    );
    runtime.kill_all();
    assert!(
        !runtime.mobile_delivery_can_capture(&second),
        "关闭后无法再证实本地工作树归属于该回合"
    );
}
