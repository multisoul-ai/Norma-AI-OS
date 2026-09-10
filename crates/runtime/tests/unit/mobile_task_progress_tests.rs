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
