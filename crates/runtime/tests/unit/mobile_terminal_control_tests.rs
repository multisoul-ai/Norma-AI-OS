use super::*;
use crate::NodeInput;
use norma::agent_hook::{parse_hook_event, HookSource};
use norma::mobile::{TerminalInput, TerminalRequest};
use serde_json::json;
use std::sync::Mutex;

pub(super) fn terminal() -> (Runtime, storage::MobileTurnKey, Arc<Mutex<Vec<Vec<u8>>>>) {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let captured = writes.clone();
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(move |_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::recording(
            tx,
            captured.clone(),
        )))
    }));
    // 原生默认80列×24行；只创建一个录制PTY，不启动真实Provider。
    let node = runtime
        .spawn_agent(
            AgentProfile::Codex,
            CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .expect("创建原终端");
    let hook = parse_hook_event(HookSource::Codex, node,
        json!({"hook_event_name":"UserPromptSubmit","session_id":"control-session","turn_id":"turn-a"}))
        .expect("真实格式的测试Hook");
    assert!(
        runtime.observe_mobile_hook(&hook),
        "必须先由Provider Hook确认回合"
    );
    let key = runtime
        .mobile_latest_turn_key(node)
        .expect("原Provider身份");
    (runtime, key, writes)
}

pub(super) fn page(runtime: &Runtime, key: &storage::MobileTurnKey) -> norma::mobile::TerminalPage {
    runtime
        .mobile_terminal_page(&TerminalRequest {
            workspace_id: "project-a".into(),
            route_id: "route-a".into(),
            node_id: key.node_id,
            run_id: key.run_id.clone(),
            session_id: Some(key.session_id.clone()),
            before: None,
            revision: None,
        })
        .expect("读取同一PTY")
}

/// 持有者独占原终端输入；竞争入口被拒绝，交还不提交或清掉人工草稿。
///
/// 数据构造：一个80×24录制PTY、一个已确认turn-a、两个不同控制操作a/b；人工文本为draft。
/// 执行过程：接管a/重复a→竞争b和普通输入→持有者输入→重放旧屏幕→交还→自动派工尝试→桌面接续。
/// 预期结果：仅人工draft与桌面接续字节到达原PTY；普通/自动/旧屏幕输入无字节，释放后草稿仍被保护。
#[test]
fn terminal_control_excludes_other_inputs_and_preserves_draft_on_return() {
    let (mut runtime, key, writes) = terminal();
    let node = key.node_id;
    let control_a = TerminalInputLease::new(key.clone()).expect("原任务租约");
    let control_b = TerminalInputLease::new(key.clone()).expect("竞争租约");
    runtime
        .claim_mobile_terminal_input(&key, &control_a)
        .expect("接管原任务终端");
    runtime
        .claim_mobile_terminal_input(&key, &control_a)
        .expect("同次接管可确认/续期");
    assert!(
        runtime.mobile_terminal_is_controlled(node),
        "接管必须反映Mac实际输入权"
    );
    assert_eq!(
        runtime.claim_mobile_terminal_input(&key, &control_b),
        Err("terminal_controlled"),
        "另一操作不能夺取输入权"
    );
    let screen = page(&runtime, &key);
    assert!(!screen.input_ready, "普通读取不能授予已被接管终端的输入权");
    assert!(
        runtime
            .send_node_input(node, NodeInput::Text("desktop".into()))
            .is_err(),
        "桌面输入必须受同一仲裁限制"
    );
    assert_eq!(
        runtime.send_mobile_terminal_input(
            &screen.card,
            screen.screen_signature,
            screen.input_sequence,
            &TerminalInput::Enter
        ),
        Err("terminal_controlled"),
        "旧手机协议不能绕过接管"
    );
    assert_eq!(
        runtime.validate_mobile_message(&screen.card, "other task"),
        Err("terminal_controlled"),
        "消息的专用粘贴路径也必须拒绝"
    );
    assert_eq!(
        runtime.validate_terminal_stop_input(node),
        Err("terminal_controlled"),
        "专用中断不能偷偷写入停止键"
    );
    assert!(
        runtime
            .send_agent_task_text(node, "automatic task")
            .is_err(),
        "自动派工必须等待交还"
    );
    assert!(
        writes.lock().expect("录制").is_empty(),
        "竞争请求不能写出任何字节"
    );
    runtime
        .send_controlled_terminal_input(
            &key,
            &control_a,
            &screen.card,
            screen.screen_signature,
            screen.input_sequence,
            &TerminalInput::Text {
                text: "draft".into(),
            },
        )
        .expect("持有者输入");
    assert_eq!(
        runtime.send_controlled_terminal_input(
            &key,
            &control_a,
            &screen.card,
            screen.screen_signature,
            screen.input_sequence,
            &TerminalInput::Enter
        ),
        Err("terminal_changed"),
        "同一屏幕的旧输入不得重放"
    );
    assert_eq!(
        runtime.release_mobile_terminal_input(&key, &control_b),
        Err("terminal_controlled"),
        "另一操作不能交还持有者输入权"
    );
    runtime
        .release_mobile_terminal_input(&key, &control_a)
        .expect("原操作交还");
    runtime
        .release_mobile_terminal_input(&key, &control_a)
        .expect("重复交还无副作用");
    assert!(
        !runtime.mobile_terminal_is_controlled(node),
        "交还后不再占用输入权"
    );
    assert!(
        runtime
            .send_agent_task_text(node, "overwrite draft")
            .is_err(),
        "交还不能允许自动派工覆盖未提交人工草稿"
    );
    runtime
        .send_node_input(node, NodeInput::Text(" continued".into()))
        .expect("桌面可处理原草稿");
    assert_eq!(
        *writes.lock().expect("录制"),
        vec![b"draft".to_vec(), b" continued".to_vec()],
        "仅持有者和交还后的桌面输入到达原PTY，没有隐式Enter/Ctrl-U"
    );
    assert_eq!(runtime.live_len(), 1, "接管不启动第二个执行器");
}

/// 输入权到期不能被旧输入/续期复活；新持有者和新会话不接受旧控制操作。
///
/// 数据构造：同一个PTY、原turn-a、人工后续turn-b；15秒租期由测试直接推进deadline至now，无sleep。
/// 执行过程：取得a→确认同session后续回合→续期a→到期→旧输入/旧续期→新b→旧a交还→SessionStart替换。
/// 预期结果：到期及换会话解除占用；旧a不能写入/复活/释放b；读写仍仅一个原PTY且无多余字节。
#[test]
fn terminal_control_expiry_and_session_change_reject_stale_owners() {
    let (mut runtime, key, writes) = terminal();
    let control_a = TerminalInputLease::new(key.clone()).expect("原任务租约");
    runtime
        .claim_mobile_terminal_input(&key, &control_a)
        .expect("接管a");
    let mut next = key.clone();
    next.turn_id = "turn-b".into();
    let control_b = TerminalInputLease::new(next.clone()).expect("后续回合租约");
    let hook = parse_hook_event(HookSource::Codex, key.node_id,
        json!({"hook_event_name":"UserPromptSubmit","session_id":key.session_id,"turn_id":next.turn_id}))
        .expect("同会话后续Hook");
    assert!(
        runtime.observe_mobile_hook(&hook),
        "后续真实回合仍被观察，不冻结进程"
    );
    runtime
        .claim_mobile_terminal_input(&key, &control_a)
        .expect("同一持有者仍以原接管身份续期");
    assert_eq!(
        runtime.claim_mobile_terminal_input(&next, &control_a),
        Err("session_changed"),
        "不能把续期改绑到另一个来源回合"
    );
    runtime
        .mobile_runs
        .get_mut(&key.node_id)
        .expect("原run")
        .native_input
        .control
        .as_mut()
        .expect("原输入权")
        .deadline = Instant::now();
    assert!(
        !runtime.mobile_terminal_is_controlled(key.node_id),
        "15秒deadline到达后必须失效"
    );
    let screen = page(&runtime, &key);
    assert_eq!(
        runtime.send_controlled_terminal_input(
            &key,
            &control_a,
            &screen.card,
            screen.screen_signature,
            screen.input_sequence,
            &TerminalInput::Enter
        ),
        Err("terminal_control_expired"),
        "过期操作不能写入或暗中续期"
    );
    assert_eq!(
        runtime.claim_mobile_terminal_input(&key, &control_a),
        Err("terminal_control_expired"),
        "迟到的原续期不能复活已过期接管"
    );
    assert_eq!(
        runtime.claim_mobile_terminal_input(&key, &control_b),
        Err("session_changed"),
        "新接管须重新核对真实当前回合，不能跟随复用节点"
    );
    runtime
        .claim_mobile_terminal_input(&next, &control_b)
        .expect("明确的新控制操作绑定当前回合");
    assert_eq!(
        runtime.release_mobile_terminal_input(&key, &control_a),
        Err("terminal_controlled"),
        "旧交还不能释放新持有者"
    );
    let restart = parse_hook_event(
        HookSource::Codex,
        key.node_id,
        json!({"hook_event_name":"SessionStart","session_id":"replacement-session"}),
    )
    .expect("替换会话");
    assert!(
        runtime.observe_mobile_hook(&restart),
        "真实SessionStart应替换会话"
    );
    assert!(
        !runtime.mobile_terminal_is_controlled(key.node_id),
        "新会话不得继承旧输入权"
    );
    assert_eq!(
        runtime.release_mobile_terminal_input(&next, &control_b),
        Err("session_changed"),
        "旧会话交还不能操作新会话"
    );
    assert!(
        writes.lock().expect("字节").is_empty(),
        "全部控制操作只改变输入权，不自动发送Enter或停止键"
    );
    assert_eq!(runtime.live_len(), 1, "会话身份变化不创建另一个执行器");
}

/// 新接管不能打断未确认输入；未接管的自动任务仍保留既有Ctrl-U清理后重试路径。
///
/// 数据构造：一个录制PTY，自动输入"first"、清理Ctrl-U为0x15、重试"retry"、人工输入"human"。
/// 执行过程：自动paste→接管被拒→原路径清理→自动重试→Provider确认→接管/人工输入→到期→自动覆盖尝试。
/// 预期结果：原自动重试能继续；到期后人工草稿阻止自动paste；控制请求不注入任何额外字节。
#[test]
fn terminal_control_draft_guard_preserves_existing_automatic_retry() {
    let (mut runtime, key, writes) = terminal();
    let node = key.node_id;
    let control_a = TerminalInputLease::new(key.clone()).expect("原任务租约");
    runtime
        .send_agent_task_text(node, "first")
        .expect("原自动输入");
    assert_eq!(
        runtime.claim_mobile_terminal_input(&key, &control_a),
        Err("desktop_input_pending"),
        "尚未提交的自动paste与Enter之间不能插入接管"
    );
    runtime
        .send_node_input(
            node,
            NodeInput::Key(terminal::KeyInput {
                key: terminal::Key::Char('u'),
                ctrl: true,
                alt: false,
                shift: false,
            }),
        )
        .expect("复用原自动提交失败的清理步骤");
    runtime
        .send_agent_task_text(node, "retry")
        .expect("未接管的既有自动清理/重试不能被新功能阻断");
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
        .expect("重试提交");
    let hook = parse_hook_event(HookSource::Codex, node,
        json!({"hook_event_name":"UserPromptSubmit","session_id":key.session_id,"turn_id":"confirmed-retry"}))
        .expect("Provider确认重试");
    assert!(runtime.observe_mobile_hook(&hook), "实际Hook确认提交");
    let key = runtime.mobile_latest_turn_key(node).expect("当前回合");
    let control_a = TerminalInputLease::new(key.clone()).expect("确认回合后的新租约");
    runtime
        .claim_mobile_terminal_input(&key, &control_a)
        .expect("提交确认后可接管");
    let screen = page(&runtime, &key);
    runtime
        .send_controlled_terminal_input(
            &key,
            &control_a,
            &screen.card,
            screen.screen_signature,
            screen.input_sequence,
            &TerminalInput::Text {
                text: "human".into(),
            },
        )
        .expect("人工输入");
    runtime
        .mobile_runs
        .get_mut(&node)
        .expect("原run")
        .native_input
        .control
        .as_mut()
        .expect("输入权")
        .deadline = Instant::now();
    assert!(
        !runtime.mobile_terminal_is_controlled(node),
        "到期应释放独占输入"
    );
    assert!(
        runtime.send_agent_task_text(node, "overwrite").is_err(),
        "租期到期不能授权覆盖人工草稿"
    );
    assert_eq!(
        *writes.lock().expect("字节"),
        vec![
            b"first".to_vec(),
            vec![0x15],
            b"retry".to_vec(),
            vec![0x0d],
            b"human".to_vec()
        ],
        "仅保留原自动重试和人工输入，没有overwrite或隐式清理"
    );
}

/// 人工Enter之后迟到的旧回合Hook不能解除草稿保护；只有新回合确认后自动派工才恢复。
///
/// 数据构造：一个PTY、原turn-a；人工正文follow-up共9字节、Enter=0x0d；重复旧Hook和新turn-b各一次。
/// 执行过程：接管→人工正文/Enter→交还→旧UserPromptSubmit迟到→尝试自动覆盖→新Hook确认→自动继续。
/// 预期结果：旧Hook不能解除等待且不产生overwrite字节；新Hook释放等待，原PTY只收到人工正文、Enter、后续任务。
#[test]
fn terminal_control_late_hook_does_not_acknowledge_new_human_input() {
    let (mut runtime, key, writes) = terminal();
    let node = key.node_id;
    let control_a = TerminalInputLease::new(key.clone()).expect("原任务租约");
    runtime
        .claim_mobile_terminal_input(&key, &control_a)
        .expect("接管");
    let screen = page(&runtime, &key);
    runtime
        .send_controlled_terminal_input(
            &key,
            &control_a,
            &screen.card,
            screen.screen_signature,
            screen.input_sequence,
            &TerminalInput::Text {
                text: "follow-up".into(),
            },
        )
        .expect("人工正文");
    let screen = page(&runtime, &key);
    runtime
        .send_controlled_terminal_input(
            &key,
            &control_a,
            &screen.card,
            screen.screen_signature,
            screen.input_sequence,
            &TerminalInput::Enter,
        )
        .expect("人工Enter");
    runtime
        .release_mobile_terminal_input(&key, &control_a)
        .expect("交还不等于Provider已接收");
    let hook = |turn: &str| {
        parse_hook_event(HookSource::Codex, node,
        json!({"hook_event_name":"UserPromptSubmit","session_id":key.session_id,"turn_id":turn})).expect("Hook")
    };
    assert!(
        runtime.observe_mobile_hook(&hook("turn-a")),
        "重复旧Hook仍可被已有观察路径处理"
    );
    assert!(
        runtime.agent_task_input_held(node),
        "旧回合不能确认刚刚提交的人工Enter"
    );
    assert!(
        runtime.send_agent_task_text(node, "overwrite").is_err(),
        "迟到Hook不能让自动任务覆盖尚未确认的人工输入"
    );
    assert!(
        runtime.observe_mobile_hook(&hook("turn-b")),
        "新的Provider回合确认人工提交"
    );
    assert!(
        !runtime.agent_task_input_held(node),
        "新回合确认后不残留人工草稿锁"
    );
    runtime
        .send_agent_task_text(node, "next-task")
        .expect("已确认输入后的自动路径可继续，忙闲仍由上层调度决定");
    assert_eq!(
        *writes.lock().expect("字节"),
        vec![b"follow-up".to_vec(), vec![0x0d], b"next-task".to_vec()],
        "只能发送人工正文、Enter和确认后的后续任务，不能写入overwrite"
    );
}
