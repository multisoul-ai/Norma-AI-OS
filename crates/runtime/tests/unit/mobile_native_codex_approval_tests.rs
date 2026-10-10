use super::*;
use norma::agent_hook::{parse_hook_event, HookSource};
use serde_json::{json, Value};

fn fixture(path: &Path) -> (Runtime, NodeId) {
    let mut r = Runtime::new_for_workspace(path);
    r.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    // 100×40 is the real Codex menu probe's terminal; geometry does not encode a budget.
    let node = r
        .spawn_agent(
            AgentProfile::Codex,
            CanvasRect::new(0., 0., 640., 400.),
            100,
            40,
        )
        .expect("原Codex卡");
    observe(&mut r, node, "UserPromptSubmit", json!({}), "exec-a");
    observe(
        &mut r,
        node,
        "PreToolUse",
        json!({"command":"printf approval"}),
        "exec-a",
    );
    observe(
        &mut r,
        node,
        "PermissionRequest",
        json!({"command":"printf approval","description":"Write marker?"}),
        "exec-a",
    );
    r.apply_agent_task_update_with_outcome(crate::AgentTaskUpdate {
        node_id: node,
        profile: AgentProfile::Codex,
        provider_event: "PermissionRequest".into(),
        to: crate::AgentTaskState::WaitingApproval {
            tool_name: Some("Bash".into()),
        },
        task_title: None,
        session_id: Some("session-a".into()),
    });
    screen(&mut r, node, "printf approval", "Environment: local");
    (r, node)
}

fn observe(r: &mut Runtime, node: NodeId, name: &str, input: Value, id: &str) {
    // New rounds use distinct Provider turn IDs; tool events retain the current round's identity.
    let turn = match r.mobile_latest_turn_key(node) {
        Some(key) if name == "UserPromptSubmit" => format!("{}-next", key.turn_id),
        Some(key) => key.turn_id,
        None => "turn-a".into(),
    };
    let mut data = json!({"hook_event_name":name,"session_id":"session-a","turn_id":turn,"tool_name":"Bash","tool_input":input});
    if name != "PermissionRequest" {
        data["tool_use_id"] = json!(id);
    }
    let event = parse_hook_event(HookSource::Codex, node, data).expect("真实Codex协议结构");
    assert!(r.observe_mobile_hook(&event), "接受原会话当前回合Hook");
}

fn screen(r: &mut Runtime, node: NodeId, command: &str, environment: &str) {
    let live = r.live.get_mut(&node).expect("原PTY");
    live.input(crate::NodeInput::Text(format!("\x1b[2J\x1b[HWould you like to run the following command?\r\n{environment}\r\nReason: Write marker?\r\n$ {command}\r\n› 1. Yes, proceed (y)\r\n2. Yes, and don't ask again for commands that start with `printf\r\napproval` (p)\r\n3. No, and tell Codex what to do differently (esc)\r\nPress enter to confirm or esc to cancel"))).expect("原生菜单");
    live.drain();
}

/// Codex缺少PermissionRequest工具ID时，只能把唯一PreToolUse准确command与已验证菜单绑定。
///
/// 数据构造：原session-a/turn-a；exec-a只有command，申请增加description；菜单为0.153.4真实三选项。
/// 执行过程：读取可操作快照 → 修改菜单命令/执行环境 → 恢复 → 电脑先输入 → 回查旧请求。
/// 预期结果：准确请求提供单次允许/拒绝；相似命令、未知环境和电脑接管后均无动作，原PTY保持唯一。
#[test]
fn native_codex_approval_requires_exact_hook_and_dialog() {
    let dir = tempfile::tempdir().expect("隔离项目");
    let (mut r, node) = fixture(dir.path());
    let approval = r.mobile_approval_snapshots(node).expect("待批").remove(0);
    assert_eq!(approval.tool, "Bash", "真实Codex原卡应可审批");
    assert_eq!(
        approval.decisions,
        [ApprovalDecision::Allow, ApprovalDecision::Deny],
        "只提供单次决定"
    );
    screen(&mut r, node, "printfapproval", "Environment: local");
    assert!(
        r.mobile_approval_snapshots(node).expect("错命令")[0]
            .decisions
            .is_empty(),
        "不得删除空格匹配不同命令"
    );
    screen(&mut r, node, "printf approval", "Environment: remote");
    assert!(
        r.mobile_approval_snapshots(node).expect("未知环境")[0]
            .decisions
            .is_empty(),
        "未验证环境不得使用相同按钮"
    );
    screen(&mut r, node, "printf approval", "Environment: local");
    assert_eq!(
        r.mobile_approval_snapshots(node).expect("恢复")[0].tool,
        "Bash",
        "准确菜单恢复操作"
    );
    r.send_node_input(node, crate::NodeInput::Text("y".into()))
        .expect("电脑接管");
    assert!(
        r.reserve_native_approval(&approval.target, ApprovalDecision::Allow)
            .is_err(),
        "电脑输入后旧手机请求失效"
    );
    assert_eq!(r.live_len(), 1, "手机不建立第二PTY");
}

/// Codex的description差异是唯一允许的字段变化，不能忽略未知权限字段或错误工具结果。
///
/// 数据构造：exec-a原输入只有command；先增加未知network权限，再新回合重建准确请求。
/// 执行过程：未知字段申请 → 读取只读快照 → 新回合正确申请并预约 → 错ID/错命令Post → 准确Post。
/// 预期结果：未知字段不出现动作；只有准确ID与完整原输入能标记执行，不能靠相似command确认。
#[test]
fn native_codex_approval_rejects_unknown_inputs_and_wrong_execution() {
    let dir = tempfile::tempdir().expect("隔离项目");
    let (mut r, node) = fixture(dir.path());
    observe(&mut r, node, "UserPromptSubmit", json!({}), "exec-a");
    observe(
        &mut r,
        node,
        "PreToolUse",
        json!({"command":"printf approval"}),
        "exec-a",
    );
    observe(
        &mut r,
        node,
        "PermissionRequest",
        json!({"command":"printf approval","description":"Write marker?","network":true}),
        "exec-a",
    );
    assert!(
        r.mobile_approval_snapshots(node).expect("未知权限")[0]
            .decisions
            .is_empty(),
        "不能静默丢弃未知字段"
    );
    observe(&mut r, node, "UserPromptSubmit", json!({}), "exec-a");
    observe(
        &mut r,
        node,
        "PreToolUse",
        json!({"command":"printf approval"}),
        "exec-a",
    );
    observe(
        &mut r,
        node,
        "PermissionRequest",
        json!({"command":"printf approval","description":"Write marker?"}),
        "exec-a",
    );
    let approval = r
        .mobile_approval_snapshots(node)
        .expect("准确申请")
        .remove(0);
    let _request = r
        .reserve_native_approval(&approval.target, ApprovalDecision::Allow)
        .expect("预约原Codex");
    let executed = r.mobile_runs[&node]
        .native_approvals
        .permission
        .as_ref()
        .expect("唯一请求")
        .executed
        .clone();
    observe(
        &mut r,
        node,
        "PostToolUse",
        json!({"command":"printf approval"}),
        "exec-b",
    );
    assert!(!executed.load(Ordering::Acquire), "不同工具不能确认");
    observe(
        &mut r,
        node,
        "PostToolUse",
        json!({"command":"printfapproval"}),
        "exec-a",
    );
    assert!(!executed.load(Ordering::Acquire), "相似命令不能确认");
    observe(
        &mut r,
        node,
        "PostToolUse",
        json!({"command":"printf approval"}),
        "exec-a",
    );
    assert!(
        executed.load(Ordering::Acquire),
        "准确Post必须确认原工具执行"
    );
}

/// Codex长命令的内核argv必须完整匹配已授权命令，未知shell或额外参数不能充当执行证据。
///
/// 数据构造：实测Codex0.153.4使用/bin/zsh -c，script含双空格以检查不做宽松规范化。
/// 执行过程：准确argv→相似正文→其他shell→额外参数→空命令分别匹配。
/// 预期结果：只有已观测的完整原命令匹配，其他组合均拒绝。
#[test]
fn native_codex_shell_evidence_requires_exact_argv() {
    use native_approval_action::matches_codex_process;
    let command = "sleep 30; printf 'a  b'";
    let argv = vec!["/bin/zsh".into(), "-c".into(), command.into()];
    assert!(
        matches_codex_process(&argv, command),
        "内核中的准确原命令必须匹配"
    );
    assert!(
        !matches_codex_process(&argv, "sleep 30; printf 'a b'"),
        "不能压缩命令空格"
    );
    let mut other = argv.clone();
    other[0] = "/bin/sh".into();
    assert!(!matches_codex_process(&other, command), "未知shell不得猜测");
    other = argv.clone();
    other[1] = "-lc".into();
    assert!(
        !matches_codex_process(&other, command),
        "未知启动选项不得猜测"
    );
    other = argv.clone();
    other.push("extra".into());
    assert!(!matches_codex_process(&other, command), "不能丢弃额外argv");
    assert!(
        !matches_codex_process(&["/bin/zsh".into(), "-c".into(), "".into()], ""),
        "空命令不是执行证据"
    );
}
