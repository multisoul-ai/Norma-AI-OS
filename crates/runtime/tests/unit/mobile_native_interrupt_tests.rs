use super::*;
use norma::agent_hook::{parse_hook_event, HookSource};
use serde_json::json;
use std::io::Write;

/// 原生Codex声明当前回合停止能力，但空会话不能构造有效停止。
///
/// 数据构造：一张mobile_control=false的640×400原卡，80×24 echo PTY，无session/turn。
/// 执行过程：读取原卡能力 → 用当前空快照申请停止。
/// 预期结果：能力为true；空回合仍被state_not_ready拒绝，不要求更换受控模式。
#[test]
fn native_codex_stop_capability_preserves_turn_validation() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    runtime
        .spawn_agent(
            AgentProfile::Codex,
            CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .expect("原卡");
    let expected = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        expected.can_interrupt,
        Some(true),
        "原生Codex应能通过准确证据停止"
    );
    assert_eq!(
        runtime.validate_mobile_interrupt(&expected),
        Err("state_not_ready"),
        "无回合不能发停止按键"
    );
    assert!(expected.turn_id.is_none(), "不能为停止功能伪造回合");
}

/// Codex只接受预约位置之后的准确turnCompleted，正文和重建状态均不能冒充中断。
///
/// 数据构造：预约位置4；旧turn-a边界4，正文7，其他turn-b边界8，新turn-a边界9。
/// 执行过程：未完成页 → 排除旧边界/正文/其他回合 → 新边界 → 自然完成 → 非法及重复边界。
/// 预期结果：只有位置9的准确边界返回interrupted，自然完成保持completed，非法/重复终态明确失败。
#[test]
fn native_codex_stop_requires_a_new_canonical_boundary() {
    let pending = json!({"data":[
        {"type":"turnCompleted","turnId":"turn-a","position":4,"status":"interrupted"},
        {"type":"item","position":7,"item":{"text":"turnCompleted interrupted"}},
        {"type":"turnCompleted","turnId":"turn-b","position":8,"status":"interrupted"}
    ]});
    assert_eq!(
        archive::timeline_boundary(&pending, "turn-a", Some(4)),
        Ok((8, None)),
        "旧边界、正文及另一回合都不能确认停止"
    );
    let mut ended = pending.clone();
    ended["data"].as_array_mut().expect("测试页").push(
        json!({"type":"turnCompleted","turnId":"turn-a","position":9,"status":"interrupted"}),
    );
    assert_eq!(
        archive::timeline_boundary(&ended, "turn-a", Some(4)),
        Ok((9, Some("interrupted"))),
        "准确新增边界才能返回中断"
    );
    ended["data"][3]["status"] = json!("completed");
    assert_eq!(
        archive::timeline_boundary(&ended, "turn-a", Some(4)),
        Ok((9, Some("completed"))),
        "自然结束不可误报中断"
    );
    ended["data"][3]["status"] = json!("inProgress");
    assert!(
        archive::timeline_boundary(&ended, "turn-a", Some(4)).is_err(),
        "活动状态不是完成证据"
    );
    ended["data"][3]["status"] = json!("interrupted");
    ended["data"]
        .as_array_mut()
        .expect("测试页")
        .push(json!({"type":"turnCompleted","turnId":"turn-a","position":10,"status":"failed"}));
    assert!(
        archive::timeline_boundary(&ended, "turn-a", Some(4)).is_err(),
        "重复矛盾终态不能任选一条"
    );
}

fn fixture(path: &Path) -> (Runtime, NodeId) {
    let mut runtime = Runtime::new_for_workspace(path);
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    // 640×400为既有卡片尺寸，80×24为测试PTY尺寸；不启动真实Provider。
    let node = runtime
        .spawn_agent(
            AgentProfile::Claude,
            CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .expect("原生卡");
    std::fs::write(path.join("session-a.jsonl"), "").expect("新历史");
    event(&mut runtime, node, path, "UserPromptSubmit", "turn-a");
    runtime.apply_agent_task_update_with_outcome(crate::AgentTaskUpdate {
        node_id: node,
        profile: AgentProfile::Claude,
        provider_event: "UserPromptSubmit".into(),
        to: crate::AgentTaskState::Thinking,
        task_title: None,
        session_id: Some("session-a".into()),
    });
    (runtime, node)
}

fn event(runtime: &mut Runtime, node: NodeId, path: &Path, name: &str, turn: &str) {
    let hook = parse_hook_event(
        HookSource::Claude,
        node,
        json!({
            "hook_event_name":name, "session_id":"session-a", "prompt_id":turn,
            "transcript_path":path.join("session-a.jsonl")
        }),
    )
    .expect("原生Hook");
    assert!(runtime.observe_mobile_hook(&hook), "本会话原生Hook应被接受");
}

fn marker(path: &Path, turn: &str) {
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(path.join("session-a.jsonl"))
        .expect("历史");
    writeln!(file, "{}", json!({"type":"user", "uuid":turn, "sessionId":"session-a", "cwd":path,
        "promptId":turn, "message":{"role":"user", "content":[{"type":"text", "text":"[Request interrupted by user]"}]}})).expect("原生中断证据");
}

/// 原生Claude卡在同一PTY停止，只有新追加的准确回合证据可以完成回执。
///
/// 数据构造：mobile_control=false、无受控执行器；session-a/turn-a，旧标记与新标记各一条。
/// 执行过程：验证能力 → 捕获EOF → 原终端Escape → 旧标记无效 → 追加准确标记 → 结束回合。
/// 预期结果：发送按键不算成功，准确证据返回interrupted；保留一个PTY、原生模式及下一条输入能力。
#[test]
fn native_claude_stop_reuses_terminal_and_waits_for_evidence() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let (mut runtime, node) = fixture(dir.path());
    let expected = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        expected.can_interrupt,
        Some(true),
        "有准确历史和回合的原生Claude应能停止"
    );
    assert_eq!(
        runtime.validate_mobile_interrupt(&expected),
        Ok(()),
        "原生卡无需重开受控会话"
    );
    marker(dir.path(), "turn-a");
    let (mut request, receipt) = runtime
        .interrupt_mobile_terminal(node, expected.run_id.as_deref().expect("原运行"), "turn-a")
        .expect("预约原生停止");
    let terminal_interrupt::InterruptHistory::Claude { reader, .. } = &mut request.history else {
        panic!("Claude fixture reader")
    };
    reader.projects_root = dir.path().into();
    request.prepare().expect("后台捕获末尾");
    runtime
        .send_mobile_terminal_interrupt(&mut request)
        .expect("同一PTY发Escape");
    assert!(receipt.try_recv().is_err(), "按键送达不能伪造完成");
    assert!(
        !request.poll().expect("检查旧标记"),
        "预约之前的中断不能当本次结果"
    );
    marker(dir.path(), "turn-a");
    assert!(request.poll().expect("新标记"), "准确新证据应确认中断");
    assert!(
        runtime.finish_mobile_terminal_interrupt(&request, None),
        "结束准确原生回合"
    );
    assert_eq!(
        receipt.try_recv().expect("回执").expect("确认")["status"],
        "interrupted",
        "只能报告实际中断"
    );
    assert_eq!(
        runtime.agent_task_state(node),
        Some(crate::AgentTaskState::Ended),
        "两端必须看到任务停止"
    );
    assert_eq!(runtime.live_len(), 1, "原生终端保持运行");
    assert!(
        matches!(&runtime.document.node(node).expect("原卡").kind, CanvasNodeKind::Agent(spec) if !spec.mobile_control),
        "原卡模式不得变更"
    );
    assert!(
        runtime.mobile_runs[&node].claude.is_none(),
        "不能暗中创建受控审批执行器"
    );
    let ended = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        runtime.validate_mobile_message(&ended, "继续讨论"),
        Ok(()),
        "系统Escape不能变成电脑未发送草稿"
    );
}

/// 预约停止后自然完成或新回合到达，不能把它误报为中断或向新回合发送Escape。
///
/// 数据构造：同一原生session-a，先turn-a自然Stop，再turn-b预约后被turn-c替换。
/// 执行过程：预约A → 原生Stop → completed回执；预约B并捕获EOF → 新回合C → 拒绝旧请求发送。
/// 预期结果：自然完成不报interrupted，新回合不被中断；两个旧请求均不残留挂起回执。
#[test]
fn native_claude_stop_distinguishes_completion_and_replaced_turn() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let (mut runtime, node) = fixture(dir.path());
    let run = runtime
        .mobile_agent_run_id(node)
        .expect("原生运行")
        .to_owned();
    let (request, receipt) = runtime
        .interrupt_mobile_terminal(node, &run, "turn-a")
        .expect("预约A");
    event(&mut runtime, node, dir.path(), "Stop", "turn-a");
    let completed = receipt.try_recv().expect("自然结束回执").expect("自然完成");
    assert_eq!(completed["status"], "completed", "不能把自然完成标成停止");
    assert!(!request.is_active(), "自然完成后旧读者应取消");
    event(&mut runtime, node, dir.path(), "UserPromptSubmit", "turn-b");
    let (mut request, receipt) = runtime
        .interrupt_mobile_terminal(node, &run, "turn-b")
        .expect("预约B");
    let terminal_interrupt::InterruptHistory::Claude { reader, .. } = &mut request.history else {
        panic!("Claude fixture reader")
    };
    reader.projects_root = dir.path().into();
    request.prepare().expect("后台捕获");
    event(&mut runtime, node, dir.path(), "UserPromptSubmit", "turn-c");
    assert!(
        runtime
            .send_mobile_terminal_interrupt(&mut request)
            .is_err(),
        "旧请求不能发按键到新回合"
    );
    assert!(
        receipt.try_recv().expect("取消旧回执").is_err(),
        "被替换的回合不应报告成功"
    );
    assert_eq!(
        runtime.mobile_runs[&node].prompt_id.as_deref(),
        Some("turn-c"),
        "新回合必须保留"
    );
    assert!(
        runtime.mobile_runs[&node].interrupt.is_none(),
        "旧预约必须释放"
    );
}
