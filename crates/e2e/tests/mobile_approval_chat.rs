use canvas::{AgentProfile, CanvasRect};
use norma::{
    agent_hook::{parse_hook_event, ApprovalDecision, HookSource},
    mobile::{TerminalInput, TerminalRequest},
};
use runtime::{AgentTaskState, AgentTaskUpdate, NodeInput, Runtime};
use serde_json::json;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

fn hook(runtime: &mut Runtime, node: u64, name: &str, turn: &str) {
    let event = parse_hook_event(
        HookSource::Claude,
        node,
        json!({
            "hook_event_name":name,"session_id":"session-a","prompt_id":turn,
            "prompt":"继续对话","tool_use_id":"tool-a","tool_name":"Bash",
            "tool_input":{"command":"true","description":"Check"}
        }),
    )
    .expect("真实Claude事件形状");
    assert!(
        runtime.observe_mobile_hook(&event),
        "准确回合事件必须接收: {name}"
    );
    let state = match name {
        "SessionStart" => AgentTaskState::Ready,
        "PermissionRequest" => AgentTaskState::WaitingApproval {
            tool_name: Some("Bash".into()),
        },
        "Stop" => AgentTaskState::Done,
        _ => AgentTaskState::Thinking,
    };
    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id: node,
        profile: AgentProfile::Claude,
        provider_event: name.into(),
        to: state,
        task_title: None,
        session_id: Some("session-a".into()),
    });
}

/// L1 手机处理 Claude 原生授权后，在同一 PTY 继续发送聊天并获得准确 Provider 回执。
///
/// 数据构造：1 张 100×40 Claude 卡（容纳完整菜单）；唯一 session-a，旧 turn-a 和新 turn-b；
///   录制 PTY 输入与注入 Provider 输出分离，不把菜单输出误当用户输入。
/// 执行过程：准确授权菜单 → 手机 Down/Up/Enter → 旧审批失效 → 工具完成 → 手机聊天粘贴/Enter
///   → UserPromptSubmit(turn-b) → 检查回执 → 真实电脑草稿 → 拒绝后续手机覆盖。
/// 预期结果：无 desktop_input_pending 误报，确认键不冒充聊天提交；正文只写一次，原 PTY/run/session 保留；
///   旧批准无法复用，真实草稿及尚未确认的手机消息均不产生伪成功。
#[test]
fn phone_can_chat_after_native_approval_navigation() {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let output = Arc::new(Mutex::new(None));
    let recorded = writes.clone();
    let sender = output.clone();
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(move |_, _, _, tx| {
        *sender.lock().expect("输出") = Some(tx.clone());
        Ok(Box::new(terminal::TestPtyBackend::recording(
            tx,
            recorded.clone(),
        )))
    }));
    let node = runtime
        .spawn_agent(
            AgentProfile::Claude,
            CanvasRect::new(0., 0., 640., 400.),
            100,
            40,
        )
        .expect("唯一原卡");
    let output = output.lock().expect("输出").take().expect("原PTY");
    hook(&mut runtime, node, "SessionStart", "turn-a");
    hook(&mut runtime, node, "UserPromptSubmit", "turn-a");
    hook(&mut runtime, node, "PreToolUse", "turn-a");
    hook(&mut runtime, node, "PermissionRequest", "turn-a");
    output.send(terminal::PtyEvent::Output(b"\x1b[2J\x1b[HBash command\r\n true\r\n Check\r\n Do you want to proceed?\r\n 1. Yes\r\n 2. Yes, always allow\r\n 3. No\r\n Esc to cancel \xc2\xb7 Tab to amend \xc2\xb7 ctrl+e to explain".to_vec())).expect("菜单绘制");
    runtime.drain_all();
    let approval = runtime
        .mobile_approval_snapshots(node)
        .expect("菜单审批")
        .remove(0);
    assert_eq!(approval.tool, "Bash", "必须先建立真实原生审批目标");
    let original = runtime.mobile_card_snapshots().remove(0);
    let scope = TerminalRequest {
        workspace_id: "workspace".into(),
        route_id: "route".into(),
        node_id: node,
        run_id: original.run_id.clone().expect("原run"),
        session_id: original.session_id.clone(),
        before: None,
        revision: None,
    };
    for key in [TerminalInput::Down, TerminalInput::Up, TerminalInput::Enter] {
        let page = runtime.mobile_terminal_page(&scope).expect("当前手机画面");
        runtime
            .send_mobile_terminal_input(
                &page.card,
                page.screen_signature,
                page.input_sequence,
                &key,
            )
            .expect("原PTY菜单操作");
    }
    assert_eq!(
        runtime.choose_mobile_approval(&approval.target, ApprovalDecision::Allow),
        Err("approval_changed"),
        "人工菜单操作必须废弃旧的批准目标"
    );
    hook(&mut runtime, node, "PostToolUse", "turn-a");
    hook(&mut runtime, node, "Stop", "turn-a");
    let completed = runtime.mobile_latest_turn_key(node).expect("原回合");
    assert!(
        runtime.mobile_delivery_can_capture(&completed),
        "菜单Enter不能被当作尚未确认的聊天而阻塞成果保存"
    );
    let card = runtime.mobile_card_snapshots().remove(0);
    let receipt = runtime
        .send_mobile_message(&card, "继续对话".into())
        .expect("授权后无需电脑操作即可继续聊天");
    assert!(receipt.try_recv().is_err(), "粘贴本身不能冒充Provider回执");
    output
        .send(terminal::PtyEvent::Output(
            "\x1b[2J\x1b[H❯ 继续对话".as_bytes().to_vec(),
        ))
        .expect("粘贴后的真实重绘");
    runtime.drain_all();
    // 110ms = 正式粘贴到 Enter 的 100ms + 10ms 调度余量，重绘之后仅提交一次。
    runtime.drain_all(); // PTY 先入网格，下一轮提交状态机观察到该重绘后才开始计时。
    std::thread::sleep(Duration::from_millis(110));
    runtime.drain_all();
    hook(&mut runtime, node, "UserPromptSubmit", "turn-b");
    let result = receipt
        .try_recv()
        .expect("提交回执")
        .expect("准确Provider确认");
    assert_eq!(
        result["turn_id"], "turn-b",
        "必须确认新回合，不能沿用授权回合"
    );
    assert_eq!(result["session_id"], "session-a", "继续原会话");
    assert_eq!(
        result["run_id"],
        json!(original.run_id),
        "不能重启Agent来绕过状态错误"
    );
    assert_eq!(
        *writes.lock().expect("输入录制"),
        vec![
            b"\x1b[B".to_vec(),
            b"\x1b[A".to_vec(),
            b"\r".to_vec(),
            "继续对话".as_bytes().to_vec(),
            b"\r".to_vec()
        ],
        "菜单按键和聊天各只写一次，没有暗中清空或重复提交"
    );
    hook(&mut runtime, node, "Stop", "turn-b");
    runtime
        .send_node_input(node, NodeInput::Text("电脑草稿".into()))
        .expect("真实草稿");
    let card = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        runtime.validate_mobile_message(&card, "不能覆盖"),
        Err("desktop_input_pending"),
        "真正的电脑草稿继续受保护"
    );
    assert_eq!(runtime.live_len(), 1, "全流程复用唯一PTY");
    runtime.kill_all();
}
