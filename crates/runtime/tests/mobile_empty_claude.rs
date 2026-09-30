use canvas::{AgentProfile, CanvasRect};
use norma::agent_hook::{parse_hook_event, HookSource};
use runtime::{AgentTaskState, AgentTaskUpdate, NodeInput, Runtime};
use serde_json::json;
use terminal::{Key, KeyInput};

fn claude() -> (Runtime, u64) {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    // 80×24是既有原生终端默认网格，640×400是测试卡片尺寸。
    let node = runtime
        .spawn_agent(
            AgentProfile::Claude,
            CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .expect("原生Claude卡");
    (runtime, node)
}

fn start(runtime: &mut Runtime, node: u64) {
    let event = parse_hook_event(
        HookSource::Claude,
        node,
        json!({"hook_event_name":"SessionStart", "session_id":"empty-session"}),
    )
    .expect("启动Hook");
    assert!(
        runtime.observe_mobile_hook(&event),
        "准确启动Hook必须被接收"
    );
    runtime.apply_agent_task_update_with_outcome(AgentTaskUpdate {
        node_id: node,
        profile: AgentProfile::Claude,
        provider_event: "SessionStart".into(),
        to: AgentTaskState::Ready,
        task_title: None,
        session_id: Some("empty-session".into()),
    });
}

/// 首次终端确认后已进入空闲Claude，手机首条消息不能被启动按键误拦截。
///
/// 数据构造：一张原生Claude卡；Down和Enter模拟首次启动菜单选择，无聊天正文。
/// 执行过程：启动确认 → SessionStart/Ready → 校验手机发送 → 输入真实桌面草稿 → 再次校验。
/// 预期结果：启动确认不残留待输入错误；真实桌面草稿仍拒绝手机覆盖。
#[test]
fn startup_confirmation_does_not_block_first_phone_message() {
    let (mut runtime, node) = claude();
    for key in [Key::Down, Key::Enter] {
        runtime
            .send_node_input(
                node,
                NodeInput::Key(KeyInput {
                    key,
                    ctrl: false,
                    alt: false,
                    shift: false,
                }),
            )
            .expect("原生启动菜单操作");
    }
    start(&mut runtime, node);
    let card = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        runtime.validate_mobile_message(&card, "你好"),
        Ok(()),
        "启动确认不能被当作尚未发送的草稿"
    );
    runtime
        .send_node_input(node, NodeInput::Text("真正的桌面草稿".into()))
        .expect("桌面输入");
    assert_eq!(
        runtime.validate_mobile_message(&card, "手机消息"),
        Err("desktop_input_pending"),
        "真实草稿必须受保护"
    );
    runtime.kill_all();
}

/// Claude已就绪但尚无聊天文件时，读取应返回空会话而非让手机重试历史错误。
///
/// 数据构造：一张原生Claude卡、有效session，SessionStart尚未提供transcript路径；没有正文。
/// 执行过程：启动 → 获取当前运行的后台读取对象 → 读取首屏 → 关闭后重读。
/// 预期结果：空消息、没有下一页、等待首次写入；关闭后的读取不能返回成功。
#[test]
fn fresh_claude_without_transcript_returns_empty_conversation() {
    let (mut runtime, node) = claude();
    start(&mut runtime, node);
    let run = runtime
        .mobile_agent_run_id(node)
        .expect("原运行")
        .to_owned();
    let read = runtime
        .mobile_claude_history(node, &run)
        .expect("新会话也必须能够读取");
    // 20条为手机首屏生产分页大小；无记录时应为0条，与页大小无关。
    let page = read.read(None, 20).expect("无聊天记录不是读取错误");
    assert!(page.messages.is_empty(), "尚未聊天不能生成假记录");
    assert!(!page.has_more, "不存在的记录不能形成无限分页");
    assert!(page.pending_tail, "应等待Provider首次写入，并保持订阅");
    runtime.kill_all();
    assert!(read.read(None, 20).is_err(), "关闭后旧读取能力必须撤销");
}

/// 空输入框中的取消、移动光标、删除和空回车不应创建不存在的桌面草稿。
///
/// 数据构造：已Ready原生Claude，无正文；逐个发送不添加文本的按键，随后输入真实草稿。
/// 执行过程：Escape → Left → Right → Backspace → Enter → 逐次手机校验 → 真实正文/删除键。
/// 预期结果：空输入始终可发送；删除键不能凭空认定真实草稿已全部删除。
#[test]
fn empty_terminal_keys_do_not_create_a_desktop_draft() {
    let (mut runtime, node) = claude();
    start(&mut runtime, node);
    let card = runtime.mobile_card_snapshots().remove(0);
    let mut press = |key| {
        runtime
            .send_node_input(
                node,
                NodeInput::Key(KeyInput {
                    key,
                    ctrl: false,
                    alt: false,
                    shift: false,
                }),
            )
            .expect("空输入框按键");
        runtime.validate_mobile_message(&card, "你好")
    };
    assert_eq!(press(Key::Escape), Ok(()), "取消不能制造草稿");
    assert_eq!(press(Key::Left), Ok(()), "向左移动不能制造草稿");
    assert_eq!(press(Key::Right), Ok(()), "向右移动不能制造草稿");
    assert_eq!(press(Key::Backspace), Ok(()), "空删除不能制造草稿");
    assert_eq!(press(Key::Enter), Ok(()), "空回车不能制造草稿");
    runtime
        .send_node_input(node, NodeInput::Text("两字".into()))
        .expect("真实草稿");
    runtime
        .send_node_input(
            node,
            NodeInput::Key(KeyInput {
                key: Key::Backspace,
                ctrl: false,
                alt: false,
                shift: false,
            }),
        )
        .expect("仅删一字");
    assert_eq!(
        runtime.validate_mobile_message(&card, "你好"),
        Err("desktop_input_pending"),
        "删除键不能误清仍存在的草稿"
    );
    runtime.kill_all();
}

/// 迟到的启动确认不能清除确认之后已经输入的下一条草稿。
///
/// 数据构造：Enter确认后输入“下一条”，再收到SessionStart；两个输入事件的序号不同。
/// 执行过程：启动Enter → 新草稿 → 迟到SessionStart/Ready → 校验手机发送。
/// 预期结果：正确绑定session，但草稿仍受保护；不把整个运行的输入状态无条件清零。
#[test]
fn late_startup_hook_preserves_a_newer_desktop_draft() {
    let (mut runtime, node) = claude();
    runtime
        .send_node_input(
            node,
            NodeInput::Key(KeyInput {
                key: Key::Enter,
                ctrl: false,
                alt: false,
                shift: false,
            }),
        )
        .expect("启动确认");
    runtime
        .send_node_input(node, NodeInput::Text("下一条".into()))
        .expect("稍后的草稿");
    start(&mut runtime, node);
    let card = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        card.session_id.as_deref(),
        Some("empty-session"),
        "准确启动身份必须接收"
    );
    assert_eq!(
        runtime.validate_mobile_message(&card, "手机消息"),
        Err("desktop_input_pending"),
        "迟到Hook不能清除确认后的草稿"
    );
    runtime.kill_all();
}

/// 重复SessionStart不能确认其后提交的真实桌面草稿。
///
/// 数据构造：已启动会话，再输入正文和Enter，随后重放同一启动Hook。
/// 执行过程：首次启动 → 正文/Enter → 重复SessionStart → 校验手机发送。
/// 预期结果：启动事件可幂等接收，但不能代替UserPromptSubmit清除实际草稿。
#[test]
fn repeated_startup_hook_does_not_acknowledge_a_real_prompt() {
    let (mut runtime, node) = claude();
    start(&mut runtime, node);
    runtime
        .send_node_input(node, NodeInput::Text("真实任务".into()))
        .expect("桌面草稿");
    runtime
        .send_node_input(
            node,
            NodeInput::Key(KeyInput {
                key: Key::Enter,
                ctrl: false,
                alt: false,
                shift: false,
            }),
        )
        .expect("提交正文");
    start(&mut runtime, node);
    let card = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        card.session_id.as_deref(),
        Some("empty-session"),
        "重复事件不改变会话身份"
    );
    assert_eq!(
        runtime.validate_mobile_message(&card, "手机消息"),
        Err("desktop_input_pending"),
        "重复启动不能冒充正文确认"
    );
    runtime.kill_all();
}
