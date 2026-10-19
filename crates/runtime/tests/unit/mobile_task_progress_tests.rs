use super::*;
use norma::agent_hook::{parse_hook_event, HookSource};
use serde_json::{json, Value};

fn runtime() -> (Runtime, NodeId) {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let node = runtime
        .spawn_agent(
            AgentProfile::Claude,
            CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .expect("原PTY");
    let event = hook(node, "UserPromptSubmit", "turn-a", json!({}));
    assert!(runtime.observe_mobile_hook(&event), "准确回合应被接受");
    (runtime, node)
}

fn hook(node: NodeId, name: &str, turn: &str, mut raw: Value) -> norma::agent_hook::AgentHookEvent {
    raw["hook_event_name"] = json!(name);
    raw["session_id"] = json!("session-a");
    raw["prompt_id"] = json!(turn);
    parse_hook_event(HookSource::Claude, node, raw).expect("Claude事件")
}

fn create(node: NodeId, id: &str) -> norma::agent_hook::AgentHookEvent {
    hook(
        node,
        "PostToolUse",
        "turn-a",
        json!({"tool_name":"TaskCreate", "tool_use_id":format!("create-{id}"),
        "tool_input":{"subject":format!("Check {id}"),"activeForm":format!("Checking {id}")},
        "tool_response":{"task":{"id":id,"subject":format!("Check {id}")}}}),
    )
}

fn update(node: NodeId, id: &str, from: &str, to: &str) -> norma::agent_hook::AgentHookEvent {
    hook(
        node,
        "PostToolUse",
        "turn-a",
        json!({"tool_name":"TaskUpdate", "tool_use_id":format!("{id}-{from}-{to}"),
        "tool_input":{"taskId":id,"status":to}, "tool_response":{"success":true,"taskId":id,
            "statusChange":{"from":from,"to":to}, "updatedFields":["status"]}}),
    )
}

/// 原卡的两步计划只跟随成功工具事件，重复/旧回合/失败工具不能伪造进度。
///
/// 数据构造：1个原PTY、2个TaskCreate；1完成+1运行=1/2，Stop前仍有1步未完。
/// 执行过程：创建 → 开始1 → 完成1 → 重复开始1 → 开始2 → 错误身份/失败完成 → Stop → 下一回合。
/// 预期结果：显示Checking 2与1/2，重复不回退；结束不能补齐步骤，新回合清空旧计划。
#[test]
fn native_task_progress_uses_confirmed_steps_and_exact_turn() {
    let (mut runtime, node) = runtime();
    assert!(
        runtime.mobile_task_progress(node).is_none(),
        "模型尚未计划时不得显示假进度"
    );
    runtime.observe_mobile_hook(&create(node, "1"));
    runtime.observe_mobile_hook(&create(node, "2"));
    let begin = update(node, "1", "pending", "in_progress");
    runtime.observe_mobile_hook(&begin);
    runtime.observe_mobile_hook(&update(node, "1", "in_progress", "completed"));
    // A new successful call can repeat an already-completed status; Claude returns no statusChange.
    let mut repeated = update(node, "1", "completed", "completed");
    repeated.raw_json["tool_response"] = json!({"success":true,"taskId":"1","updatedFields":[]});
    runtime.observe_mobile_hook(&repeated);
    runtime.observe_mobile_hook(&begin);
    runtime.observe_mobile_hook(&update(node, "2", "pending", "in_progress"));
    let progress = runtime.mobile_task_progress(node).expect("完整计划");
    assert_eq!(progress.completed, 1, "旧工具重播不能把已完成步骤回退");
    assert_eq!(progress.total, 2, "工具调用次数不是步骤数");
    assert_eq!(
        progress.current_step.as_deref(),
        Some("Checking 2"),
        "显示真实正在执行的阶段"
    );
    let mut wrong = update(node, "2", "in_progress", "completed");
    wrong.turn_id = Some("old-turn".into());
    assert!(!runtime.observe_mobile_hook(&wrong), "旧回合不能推进步骤");
    wrong.turn_id = Some("turn-a".into());
    wrong.session_id = Some("old-session".into());
    assert!(!runtime.observe_mobile_hook(&wrong), "旧会话不能推进步骤");
    let mut failed = update(node, "2", "in_progress", "completed");
    failed.raw_json["tool_response"]["success"] = json!(false);
    runtime.observe_mobile_hook(&failed);
    assert_eq!(
        runtime.mobile_task_progress(node),
        Some(progress.clone()),
        "失败工具不得计为完成"
    );
    runtime.observe_mobile_hook(&hook(node, "Stop", "turn-a", json!({})));
    assert_eq!(
        runtime.mobile_task_progress(node),
        Some(progress),
        "模型结束不能自动补齐未完成步骤"
    );
    runtime.observe_mobile_hook(&hook(node, "UserPromptSubmit", "turn-b", json!({})));
    assert!(
        runtime.mobile_task_progress(node).is_none(),
        "下一回合不能继承上一轮数字"
    );
    assert_eq!(runtime.live_len(), 1, "不能另开执行器");
}

/// 不完整或超界的计划保持未知，避免展示局部计划为完整进度。
///
/// 数据构造：未知任务99；不连续pending→completed回执；65=64步上限+1。
/// 执行过程：先创建1 → 收到未知ID/乱序转移 → 应隐藏；另一起点创建65步 → 应隐藏。
/// 预期结果：已知计划曾可见，缺口/溢出后不可见，不能显示错误的1/1或64/64。
#[test]
fn native_task_progress_hides_gaps_and_bounds_memory() {
    let (mut runtime, node) = runtime();
    runtime.observe_mobile_hook(&create(node, "1"));
    assert_eq!(
        runtime.mobile_task_progress(node).expect("已知计划").total,
        1,
        "创建已确认"
    );
    runtime.observe_mobile_hook(&update(node, "99", "pending", "completed"));
    assert!(
        runtime.mobile_task_progress(node).is_none(),
        "未知任务意味着计划不完整"
    );
    runtime.observe_mobile_hook(&create(node, "2"));
    assert!(
        runtime.mobile_task_progress(node).is_none(),
        "不能在同一不完整回合恢复假总数"
    );
    let (mut runtime, node) = self::runtime();
    runtime.observe_mobile_hook(&create(node, "1"));
    runtime.observe_mobile_hook(&update(node, "1", "in_progress", "completed"));
    assert!(
        runtime.mobile_task_progress(node).is_none(),
        "漏掉开始事件不得计为确认完成"
    );
    let (mut runtime, node) = self::runtime();
    for id in 1..=65 {
        runtime.observe_mobile_hook(&create(node, &id.to_string()));
    }
    assert!(
        runtime.mobile_task_progress(node).is_none(),
        "65=64上限+1，不可继续积累或截成假总数"
    );
}

/// 未验证的智能体即使收到合成的完整清单，也不产生待办进度。
///
/// 数据构造：除 Claude 外的 9 个 AgentProfile 各 1 张卡。清单是合成探针，不是捕获到的
/// Hook 样例：2 项，第 1 项 status=completed，第 2 项 status=in_progress，activeForm 为
/// "Checking B"。若被误接入，已完成数/总数应为 1/2，进行中项为 "Checking B"。
/// 执行过程：每张卡先接受 UserPromptSubmit（turn-a），再投递该 profile 候选工具的 PostToolUse。
/// 预期结果：
///   - 正断言：mobile_task_progress 与卡片快照 task_progress 都是 None。
///   - 负断言：不得出现 1/2 或进行中项 "Checking B"。
///   - Codex 与 Cursor 的 Hook 必须被接受，证明断言打在待办观测上，而不是事件被提前丢掉。
#[test]
fn unverified_profiles_do_not_publish_task_progress() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    // 9 = AgentProfile 全集 10 − Claude。Claude 的 TaskCreate/TaskUpdate/TodoWrite 不在本测试改动范围内。
    let cases = [
        (AgentProfile::Codex, HookSource::Codex, "update_plan"),
        (AgentProfile::Cursor, HookSource::Cursor, "TodoWrite"),
        (AgentProfile::OpenCode, HookSource::OpenCode, "todowrite"),
        (AgentProfile::Kimi, HookSource::Kimi, "TodoWrite"),
        (AgentProfile::Pi, HookSource::Pi, "todo"),
        (AgentProfile::Grok, HookSource::Grok, "TodoWrite"),
        (AgentProfile::InfCode, HookSource::InfCode, "todo_write"),
        (AgentProfile::ZCode, HookSource::ZCode, "TodoWrite"),
        (AgentProfile::Hermes, HookSource::Hermes, "todo"),
    ];
    assert_eq!(cases.len(), 9, "未验证 profile 应为 9 个");
    let forged = json!({
        "todos": [
            {"content": "Check A", "status": "completed", "activeForm": "Checking A"},
            {"content": "Check B", "status": "in_progress", "activeForm": "Checking B"}
        ]
    });
    let forged_progress = norma::mobile::TaskProgress {
        completed: 1,
        total: 2,
        current_step: Some("Checking B".into()),
    };
    for (profile, source, tool) in cases {
        let node = runtime
            .spawn_agent(profile, CanvasRect::new(0., 0., 640., 400.), 80, 24)
            .unwrap_or_else(|_| panic!("{profile:?} 原卡应能创建"));
        let submit = other_hook(node, source, "UserPromptSubmit", "turn-a", tool, json!({}));
        assert!(
            runtime.observe_mobile_hook(&submit),
            "{profile:?} 的回合起点应被接受"
        );
        let listed = other_hook(
            node,
            source,
            "PostToolUse",
            "turn-a",
            tool,
            json!({
                "tool_input": forged,
                "tool_response": forged,
            }),
        );
        let accepted = runtime.observe_mobile_hook(&listed);
        if matches!(profile, AgentProfile::Codex | AgentProfile::Cursor) {
            assert!(
                accepted,
                "{profile:?} 的 PostToolUse 应进入待办观测；拒绝意味着本测试没有覆盖解析路径"
            );
        }
        assert!(
            runtime.mobile_task_progress(node).is_none(),
            "{profile:?} 未验证，不得把合成清单显示为待办进度"
        );
        assert_ne!(
            runtime.mobile_task_progress(node),
            Some(forged_progress.clone()),
            "{profile:?} 不得显示合成清单的 1/2 与 Checking B"
        );
        let card = runtime
            .mobile_card_snapshots()
            .into_iter()
            .find(|card| card.node_id == node)
            .unwrap_or_else(|| panic!("{profile:?} 应有卡片快照"));
        assert!(
            card.task_progress.is_none(),
            "{profile:?} 卡片快照 task_progress 也应为空"
        );
        assert_ne!(
            card.task_progress,
            Some(forged_progress.clone()),
            "{profile:?} 卡片快照不得显示合成的 1/2"
        );
    }
}

fn other_hook(
    node: NodeId,
    source: HookSource,
    name: &str,
    turn: &str,
    tool: &str,
    mut raw: Value,
) -> norma::agent_hook::AgentHookEvent {
    raw["hook_event_name"] = json!(name);
    raw["session_id"] = json!("session-a");
    raw["conversation_id"] = json!("session-a");
    raw["turn_id"] = json!(turn);
    raw["turnId"] = json!(turn);
    raw["prompt_id"] = json!(turn);
    raw["generation_id"] = json!(turn);
    raw["tool_name"] = json!(tool);
    raw["toolName"] = json!(tool);
    parse_hook_event(source, node, raw).unwrap_or_else(|_| panic!("{name} 应能解析"))
}
