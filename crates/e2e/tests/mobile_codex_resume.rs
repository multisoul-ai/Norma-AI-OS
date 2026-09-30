use canvas::{AgentProfile, CanvasRect};
use norma::agent_hook::{parse_hook_event, HookSource};
use runtime::{AgentTaskState, AgentTaskUpdate, NodeInput, Runtime};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use terminal::{Key, KeyInput, PtyEvent};

fn key(key: Key) -> NodeInput {
    NodeInput::Key(KeyInput {
        key,
        ctrl: false,
        alt: false,
        shift: false,
    })
}

fn hook(
    runtime: &mut Runtime,
    node: u64,
    name: &str,
    session: &str,
    turn: Option<&str>,
    prompt: Option<&str>,
) -> bool {
    let event = parse_hook_event(
        HookSource::Codex,
        node,
        json!({
            "hook_event_name": name, "session_id": session, "turn_id": turn,
            "source": "resume", "prompt": prompt,
        }),
    )
    .expect("真实Codex Hook形状应可解析");
    if !runtime.observe_mobile_hook(&event) {
        return false;
    }
    let to = match name {
        "SessionStart" => AgentTaskState::Ready,
        "UserPromptSubmit" => AgentTaskState::Thinking,
        "Stop" => AgentTaskState::Done,
        _ => panic!("测试只构造明确的会话/任务事件"),
    };
    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id: node,
        profile: AgentProfile::Codex,
        provider_event: name.into(),
        to,
        session_id: Some(session.into()),
        task_title: prompt.map(str::to_owned),
    });
    true
}

struct Card {
    runtime: Runtime,
    node: u64,
    output: crossbeam_channel::Sender<PtyEvent>,
}
impl Card {
    fn new() -> Self {
        let sender = Arc::new(Mutex::new(None));
        let captured = sender.clone();
        let mut runtime = Runtime::new_for_workspace("/tmp");
        runtime.set_pty_factory(Box::new(move |_, _, _, tx| {
            *captured.lock().expect("保存唯一PTY输出端") = Some(tx.clone());
            Ok(Box::new(terminal::TestPtyBackend::recording(
                tx,
                Arc::default(),
            )))
        }));
        // 55×21为现场卡499网格；640×400仅为画布创建尺寸。
        let node = runtime
            .spawn_agent(
                AgentProfile::Codex,
                CanvasRect::new(0., 0., 640., 400.),
                55,
                21,
            )
            .expect("创建唯一原卡");
        let output = sender
            .lock()
            .expect("读取输出端")
            .take()
            .expect("PTY已创建");
        let mut card = Self {
            runtime,
            node,
            output,
        };
        card.screen("model: gpt-6-astra\r\n› Ask Codex to do anything\r\ngpt-6-astra xhigh · /tmp");
        card
    }
    fn screen(&mut self, text: &str) {
        self.output
            .send(PtyEvent::Output(
                format!("\x1b[?2004h\x1b[2J\x1b[H{text}").into_bytes(),
            ))
            .expect("仅模拟Provider输出，不算桌面草稿");
        self.runtime.drain_all();
    }
    fn input(&mut self, input: NodeInput) {
        self.runtime
            .send_node_input(self.node, input)
            .expect("输入原PTY");
    }
    fn resume(&mut self) {
        self.input(NodeInput::Text("/resume".into()));
        self.input(key(Key::Enter));
    }
    fn resumed_screen(&mut self) {
        // 121 = 检测尾部上限120 + 1；恢复的历史把启动model头部挤出检测范围。
        self.screen(&format!(
            "{}› Ask Codex to do anything\r\ngpt-6-astra xhigh · /tmp",
            "restored conversation\r\n".repeat(121)
        ));
    }
}

/// 已有会话的原生Codex通过/resume选择历史后，手机可继续发送且只绑定真实Hook会话。
///
/// 数据构造：1张55×21原卡、旧session-a/turn-a、新session-b/turn-b；恢复121行历史。
/// 执行过程：旧回合完成 → /resume菜单 → Enter选择 → 恢复画面（不发SessionStart）
///   → 拒绝旧操作/迟到Stop → 手机首次发送 → Codex延迟SessionStart和UserPromptSubmit。
/// 预期结果：菜单不可发送，恢复后ready且可发送；旧run失效，新会话由Hook确认；
///   回执绑定session-b/turn-b且不丢失，原PTY始终只有1个。
#[test]
fn mobile_codex_resume_rebinds_after_first_message_without_losing_receipt() {
    let mut card = Card::new();
    assert!(
        hook(
            &mut card.runtime,
            card.node,
            "SessionStart",
            "session-a",
            None,
            None
        ),
        "登记旧会话"
    );
    assert!(
        hook(
            &mut card.runtime,
            card.node,
            "UserPromptSubmit",
            "session-a",
            Some("turn-a"),
            Some("old task")
        ),
        "登记旧任务"
    );
    assert!(
        hook(
            &mut card.runtime,
            card.node,
            "Stop",
            "session-a",
            Some("turn-a"),
            None
        ),
        "旧任务完成"
    );
    let old = card.runtime.mobile_card_snapshots().remove(0);
    card.resume();
    card.screen("Resume a session\r\n› previous conversation\r\nEnter to resume · Esc to cancel");
    let menu = card.runtime.mobile_card_snapshots().remove(0);
    assert_ne!(menu.state, "ready", "恢复选择菜单不得允许任务消息");
    assert!(
        card.runtime
            .validate_mobile_message(&menu, "continue")
            .is_err(),
        "菜单必须拒绝任务输入"
    );
    card.input(key(Key::Down));
    card.input(key(Key::Enter));
    card.resumed_screen();
    let resumed = card.runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        resumed.state, "ready",
        "恢复完成不能因缺少启动model头部保持未就绪"
    );
    assert_ne!(
        resumed.run_id, old.run_id,
        "切换必须使旧手机操作的运行身份失效"
    );
    assert!(
        resumed.session_id.is_none(),
        "延迟Hook前不得冒用旧会话或猜测新会话ID"
    );
    assert_eq!(
        card.runtime.validate_mobile_message(&resumed, "continue"),
        Ok(()),
        "已消费的/resume不能残留为桌面草稿"
    );
    assert_eq!(
        card.runtime.validate_mobile_message(&old, "stale"),
        Err("card_changed"),
        "旧操作不能进入恢复后的会话"
    );
    assert!(
        !hook(
            &mut card.runtime,
            card.node,
            "Stop",
            "session-a",
            Some("turn-a"),
            None
        ),
        "旧回合迟到Stop不得绑定或结束新会话"
    );
    assert!(
        !hook(
            &mut card.runtime,
            card.node,
            "UserPromptSubmit",
            "session-a",
            Some("turn-a"),
            Some("old task")
        ),
        "旧回合迟到提交不得重新绑定原会话"
    );
    let receipt = card
        .runtime
        .send_mobile_message(&resumed, "continue".into())
        .expect("手机发往原PTY");
    // 110ms = 正式paste→Enter等待100ms + 10ms调度余量；先drain确认重绘，再等待Enter。
    card.screen("› continue\r\ngpt-6-astra xhigh · /tmp");
    card.runtime.drain_all();
    std::thread::sleep(Duration::from_millis(110));
    card.runtime.drain_all();
    assert!(
        hook(
            &mut card.runtime,
            card.node,
            "SessionStart",
            "session-b",
            None,
            None
        ),
        "首次普通消息触发真实新会话Hook"
    );
    assert!(
        hook(
            &mut card.runtime,
            card.node,
            "UserPromptSubmit",
            "session-b",
            Some("turn-b"),
            Some("continue")
        ),
        "精确Provider提交应接受"
    );
    let received = receipt
        .try_recv()
        .expect("Provider应返回回执")
        .expect("会话切换不得丢失手机回执");
    assert_eq!(
        received["session_id"], "session-b",
        "回执必须绑定真实恢复会话"
    );
    assert_eq!(received["turn_id"], "turn-b", "回执必须绑定首次新回合");
    assert_eq!(
        card.runtime.mobile_card_snapshots()[0].run_id,
        resumed.run_id,
        "延迟Hook不能再次旋转运行身份或取消正在提交的消息"
    );
    assert_eq!(card.runtime.live_len(), 1, "恢复前后只能使用唯一原PTY");
}

/// 新建空卡/resume后无需先在桌面发消息，连续第二次恢复仍保持手机可输入。
///
/// 数据构造：1张55×21空卡，没有任何SessionStart；两次分别用/resume和/resume UUID。
/// 执行过程：空卡 → 选择历史 → ready → 再恢复另一历史 → ready → 校验手机输入。
/// 预期结果：每次恢复轮换run但不猜session；首次/第二次恢复都可发送，第一份快照失效。
#[test]
fn mobile_empty_codex_can_resume_twice_before_any_task_hook() {
    let mut card = Card::new();
    card.resume();
    card.screen("Resume a session\r\nEnter to resume · Esc to cancel");
    card.input(key(Key::Enter));
    card.resumed_screen();
    let first = card.runtime.mobile_card_snapshots().remove(0);
    assert_eq!(first.state, "ready", "空卡/resume后不应卡在未知状态");
    assert_eq!(
        card.runtime.validate_mobile_message(&first, "first"),
        Ok(()),
        "无需先在桌面提交普通消息"
    );
    card.input(NodeInput::Text(
        "/resume 01a09881-b00f-76f3-8ef3-38973481c02f".into(),
    ));
    card.input(key(Key::Enter));
    card.resumed_screen();
    let second = card.runtime.mobile_card_snapshots().remove(0);
    assert_eq!(second.state, "ready", "直接指定会话的/resume也应恢复就绪");
    assert_ne!(
        second.run_id, first.run_id,
        "连续切换不能复用第一次恢复的操作身份"
    );
    assert!(
        second.session_id.is_none(),
        "即使参数是UUID也必须等待Provider确认绑定"
    );
    assert_eq!(
        card.runtime.validate_mobile_message(&second, "second"),
        Ok(()),
        "第二次/resume也不能残留桌面草稿"
    );
    assert_eq!(
        card.runtime.validate_mobile_message(&first, "stale"),
        Err("card_changed"),
        "第一次恢复的过期操作必须拒绝"
    );
}

/// 取消/resume只返回原会话，不丢掉已知绑定，也不能继续复用菜单前的操作快照。
///
/// 数据构造：1张原卡，session-a已通过真实Hook绑定；Escape是唯一取消键。
/// 执行过程：/resume菜单 → Escape → 原空输入框重绘 → 手机校验。
/// 预期结果：仍绑定session-a且可发送；旧运行快照失效，不创建第二个PTY。
#[test]
fn mobile_codex_cancel_resume_preserves_original_session() {
    let mut card = Card::new();
    assert!(
        hook(
            &mut card.runtime,
            card.node,
            "SessionStart",
            "session-a",
            None,
            None
        ),
        "原会话已就绪"
    );
    let original = card.runtime.mobile_card_snapshots().remove(0);
    card.resume();
    card.screen("Resume a session\r\nEnter to resume · Esc to cancel");
    card.input(key(Key::Escape));
    card.screen("› Ask Codex to do anything\r\ngpt-6-astra xhigh · /tmp");
    let cancelled = card.runtime.mobile_card_snapshots().remove(0);
    assert_eq!(cancelled.state, "ready", "取消菜单后应回到可输入状态");
    assert_eq!(
        cancelled.session_id.as_deref(),
        Some("session-a"),
        "取消不得清掉原会话绑定"
    );
    assert_eq!(
        card.runtime.validate_mobile_message(&cancelled, "continue"),
        Ok(()),
        "取消/resume不应残留草稿阻塞"
    );
    assert_ne!(
        cancelled.run_id, original.run_id,
        "进入过切换菜单的旧手机操作必须重新确认目标"
    );
    assert_eq!(card.runtime.live_len(), 1, "取消不能更换终端");
}

/// 恢复选择后马上键入的桌面草稿不能被迟到重绘或SessionStart清除。
///
/// 数据构造：1张空卡；选择Enter后新增字符串private draft（13个ASCII字符），无新Enter。
/// 执行过程：选择历史 → 键入草稿 → 到达旧的空输入框重绘 → 到达SessionStart。
/// 预期结果：旧重绘不能宣称可发送；新会话Hook确认后仍以desktop_input_pending保护草稿。
#[test]
fn mobile_codex_resume_preserves_draft_typed_after_selection() {
    let mut card = Card::new();
    card.resume();
    card.screen("Resume a session\r\nEnter to resume · Esc to cancel");
    card.input(key(Key::Enter));
    card.input(NodeInput::Text("private draft".into()));
    card.resumed_screen();
    let waiting = card.runtime.mobile_card_snapshots().remove(0);
    assert_ne!(
        waiting.state, "ready",
        "草稿未确认时不能用迟到空白画面判定就绪"
    );
    assert!(
        card.runtime
            .validate_mobile_message(&waiting, "phone")
            .is_err(),
        "迟到重绘不得放行竞争手机输入"
    );
    assert!(
        hook(
            &mut card.runtime,
            card.node,
            "SessionStart",
            "session-b",
            None,
            None
        ),
        "真实会话Hook可以确认身份"
    );
    let current = card.runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        current.session_id.as_deref(),
        Some("session-b"),
        "真实Hook必须更新会话"
    );
    assert_eq!(
        card.runtime.validate_mobile_message(&current, "phone"),
        Err("desktop_input_pending"),
        "SessionStart不得清除选择之后的新草稿"
    );
}
