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

fn todo_item(content: &str, status: &str, active: &str) -> Value {
    json!({ "content": content, "status": status, "activeForm": active })
}

/// 形状来自本机 Claude Code 2.1.296 的 TodoWrite 实现与 PostToolUse 构造：
/// `tool_input.todos` 与 `tool_response.newTodos` 同为整份替换列表，
/// 项为 content/status/activeForm；`oldTodos` 是工具内部上一份，全部完成后会被清空。
fn todo_write(
    node: NodeId,
    id: &str,
    old: Value,
    todos: Value,
) -> norma::agent_hook::AgentHookEvent {
    hook(
        node,
        "PostToolUse",
        "turn-a",
        json!({
            "tool_name": "TodoWrite",
            "tool_use_id": id,
            "tool_input": { "todos": todos },
            "tool_response": { "oldTodos": old, "newTodos": todos }
        }),
    )
}

/// Claude TodoWrite 用最近一次双方一致的完整清单显示已完成数、总数和进行中项。
///
/// 数据构造：3 项，已完成 1、进行中 1、未开始 1。进行中项的 activeForm 为 "Writing test"。
/// 执行过程：写入第一份清单 → 卡片快照读取 → 用新 tool_use_id 整份替换为 2/3 → 重放同一事件 → success=false 的 3/3。
/// 预期结果：先是 1/3 且当前项为 Writing test；替换后为 2/3 且当前项为 Implementing；重放和失败回执都不改变数字。
#[test]
fn claude_todo_write_progress_uses_latest_confirmed_list() {
    let (mut runtime, node) = runtime();
    let first = json!([
        todo_item("Read spec", "completed", "Reading spec"),
        todo_item("Write test", "in_progress", "Writing test"),
        todo_item("Implement", "pending", "Implementing")
    ]);
    runtime.observe_mobile_hook(&todo_write(node, "todo-1", json!([]), first));
    let progress = runtime
        .mobile_task_progress(node)
        .expect("完整 TodoWrite 清单");
    assert_eq!(progress.completed, 1, "3 项里只有 Read spec 已完成");
    assert_eq!(progress.total, 3, "总数是清单长度，不是工具调用次数");
    assert_eq!(
        progress.current_step.as_deref(),
        Some("Writing test"),
        "进行中项取 activeForm"
    );
    assert_ne!(
        progress.current_step.as_deref(),
        Some("Write test"),
        "进行中项不是祈使 content"
    );
    let card = runtime
        .mobile_card_snapshots()
        .into_iter()
        .find(|card| card.node_id == node)
        .expect("原卡快照");
    assert_eq!(
        card.task_progress,
        Some(progress),
        "卡片快照 task_progress 与 Runtime 进度相同"
    );
    let second = json!([
        todo_item("Read spec", "completed", "Reading spec"),
        todo_item("Write test", "completed", "Writing test"),
        todo_item("Implement", "in_progress", "Implementing")
    ]);
    let replacement = todo_write(node, "todo-2", json!([]), second);
    runtime.observe_mobile_hook(&replacement);
    runtime.observe_mobile_hook(&replacement);
    let progress = runtime
        .mobile_task_progress(node)
        .expect("替换后的完整清单");
    assert_eq!(progress.completed, 2, "最近一次完整清单有 2 项完成");
    assert_eq!(progress.total, 3, "替换后总数仍是 3，不与上一份相加");
    assert_eq!(
        progress.current_step.as_deref(),
        Some("Implementing"),
        "进行中项改为 Implement 的 activeForm"
    );
    let mut rejected = todo_write(
        node,
        "todo-3",
        json!([]),
        json!([
            todo_item("Read spec", "completed", "Reading spec"),
            todo_item("Write test", "completed", "Writing test"),
            todo_item("Implement", "completed", "Implementing")
        ]),
    );
    rejected.raw_json["tool_response"]["success"] = json!(false);
    runtime.observe_mobile_hook(&rejected);
    assert_eq!(
        runtime.mobile_task_progress(node),
        Some(progress),
        "tool_response.success=false 的 TodoWrite 不得记成 3/3"
    );
}

fn one_todo(status: &str) -> Value {
    json!([todo_item("Ship fix", status, "Shipping fix")])
}

/// 子智能体的 TodoWrite 不计入主回合，主清单保持原数字。
///
/// 数据构造：主清单 1/1 已完成；子清单 agent_id=worker-1，内容是另一项 pending。
/// 执行过程：只有子清单 → 主清单 → 再来一份不同的子清单。
/// 预期结果：只有子清单时进度为空；主清单保持 completed=1、total=1，不被子清单改成 0/1。
#[test]
fn claude_todo_write_ignores_subagent_lists() {
    let (mut runtime, node) = runtime();
    let mut subagent = todo_write(node, "sub-1", json!([]), one_todo("pending"));
    subagent.raw_json["agent_id"] = json!("worker-1");
    runtime.observe_mobile_hook(&subagent);
    assert!(
        runtime.mobile_task_progress(node).is_none(),
        "只有子智能体清单时主回合没有待办进度"
    );
    runtime.observe_mobile_hook(&todo_write(
        node,
        "main-1",
        json!([]),
        one_todo("completed"),
    ));
    let progress = runtime.mobile_task_progress(node).expect("主回合清单");
    assert_eq!(progress.completed, 1, "主清单的唯一项已完成");
    assert_eq!(progress.total, 1, "主清单只有 1 项");
    subagent.raw_json["tool_use_id"] = json!("sub-2");
    runtime.observe_mobile_hook(&subagent);
    assert_eq!(
        runtime.mobile_task_progress(node),
        Some(progress),
        "子智能体清单不能改写主回合的 1/1"
    );
}

/// 清单对不上、缺字段或超过 64 项时隐藏，直到新回合。
///
/// 数据构造：输入 status=completed，回执 newTodos status=pending；另一份 65=64+1 项。
/// 执行过程：先显示 1 项 → 收到不一致回执 → 同回合再给一份一致清单 → 新回合后重新写入。
/// 预期结果：缺口后进度为空，同回合不能恢复；新回合的 0/1 可以显示。65 项不能截成 64。
#[test]
fn claude_todo_write_hides_unconfirmed_lists_until_next_turn() {
    let (mut runtime, node) = runtime();
    runtime.observe_mobile_hook(&todo_write(
        node,
        "todo-1",
        json!([]),
        one_todo("completed"),
    ));
    assert_eq!(
        runtime
            .mobile_task_progress(node)
            .expect("已确认清单")
            .completed,
        1,
        "缺口前可以显示 1/1"
    );
    let mut gap = todo_write(node, "todo-2", json!([]), one_todo("completed"));
    gap.raw_json["tool_response"]["newTodos"][0]["status"] = json!("pending");
    runtime.observe_mobile_hook(&gap);
    assert!(
        runtime.mobile_task_progress(node).is_none(),
        "输入与 newTodos 不一致时不能显示 1/1"
    );
    runtime.observe_mobile_hook(&todo_write(
        node,
        "todo-3",
        json!([]),
        one_todo("in_progress"),
    ));
    assert!(
        runtime.mobile_task_progress(node).is_none(),
        "同一不完整回合不能用后一份清单恢复数字"
    );
    runtime.observe_mobile_hook(&hook(node, "UserPromptSubmit", "turn-b", json!({})));
    let mut fresh = todo_write(node, "todo-4", json!([]), one_todo("in_progress"));
    fresh.turn_id = Some("turn-b".into());
    fresh.raw_json["prompt_id"] = json!("turn-b");
    runtime.observe_mobile_hook(&fresh);
    let progress = runtime.mobile_task_progress(node).expect("新回合清单");
    assert_eq!(progress.completed, 0, "新回合不继承上一回合的已完成数");
    assert_eq!(progress.total, 1, "新回合只计算这份清单");
    assert_eq!(
        progress.current_step.as_deref(),
        Some("Shipping fix"),
        "新回合进行中项来自这份清单"
    );

    let (mut runtime, node) = self::runtime();
    let items: Vec<_> = (1..=65)
        .map(|index| {
            todo_item(
                &format!("Step {index}"),
                "pending",
                &format!("Doing {index}"),
            )
        })
        .collect();
    runtime.observe_mobile_hook(&todo_write(node, "todo-65", json!([]), json!(items)));
    assert!(
        runtime.mobile_task_progress(node).is_none(),
        "65=64上限+1，不能截成 64 后当作完整清单"
    );
}

/// TodoWrite 与 TaskCreate/TaskUpdate 不能拼成一份清单，两种顺序都隐藏。
///
/// 数据构造：TaskCreate 1 项；TodoWrite 另 1 项 completed。两套编号和字段不能对齐。
/// 执行过程：先 TaskCreate 再 TodoWrite；另一起点先 TodoWrite 再 TaskCreate。
/// 预期结果：两种顺序最终都没有进度，不能显示 1/1 或 1/2。
#[test]
fn claude_todo_write_hides_when_mixed_with_task_tools() {
    let (mut runtime, node) = runtime();
    runtime.observe_mobile_hook(&create(node, "1"));
    assert_eq!(
        runtime.mobile_task_progress(node).expect("任务清单").total,
        1,
        "混合前 TaskCreate 仍可用"
    );
    runtime.observe_mobile_hook(&todo_write(
        node,
        "todo-1",
        json!([]),
        one_todo("completed"),
    ));
    assert!(
        runtime.mobile_task_progress(node).is_none(),
        "TaskCreate 之后的 TodoWrite 不能拼成 1/2 或改成 1/1"
    );

    let (mut runtime, node) = self::runtime();
    runtime.observe_mobile_hook(&todo_write(
        node,
        "todo-1",
        json!([]),
        one_todo("completed"),
    ));
    runtime.observe_mobile_hook(&create(node, "1"));
    assert!(
        runtime.mobile_task_progress(node).is_none(),
        "TodoWrite 之后的 TaskCreate 不能把两套清单加在一起"
    );
    assert_ne!(
        runtime
            .mobile_task_progress(node)
            .map(|progress| progress.total),
        Some(1),
        "隐藏后不能留下其中一份清单的总数"
    );
}

/// 没有待办工具的回合不产生待办进度，全部完成时只显示 n/n。
///
/// 数据构造：Bash 一次；另一回合两份 TodoWrite，第二份两项都是 completed。
/// 执行过程：只观察 Bash → 快照；再观察全部完成的清单 → Stop。
/// 预期结果：Bash 回合快照没有 task_progress；完成清单是 2/2 且没有进行中项，Stop 不补新项。
#[test]
fn claude_turns_without_todo_tools_have_no_task_progress() {
    let (mut runtime, node) = runtime();
    runtime.observe_mobile_hook(&hook(
        node,
        "PostToolUse",
        "turn-a",
        json!({
            "tool_name": "Bash",
            "tool_use_id": "bash-1",
            "tool_input": { "command": "pwd" },
            "tool_response": { "stdout": "/tmp" }
        }),
    ));
    let card = runtime
        .mobile_card_snapshots()
        .into_iter()
        .find(|card| card.node_id == node)
        .expect("原卡快照");
    assert!(card.task_progress.is_none(), "只用 Bash 的回合没有待办进度");
    assert!(
        runtime.mobile_task_progress(node).is_none(),
        "Runtime 进度同样为空"
    );

    let done = json!([
        todo_item("Read spec", "completed", "Reading spec"),
        todo_item("Write test", "completed", "Writing test")
    ]);
    runtime.observe_mobile_hook(&todo_write(node, "todo-done", json!([]), done));
    runtime.observe_mobile_hook(&hook(node, "Stop", "turn-a", json!({})));
    let progress = runtime.mobile_task_progress(node).expect("已完成清单");
    assert_eq!(progress.completed, 2, "两项都已完成");
    assert_eq!(progress.total, 2, "总数仍是清单长度");
    assert!(
        progress.current_step.is_none(),
        "没有 in_progress 或 pending 时不编造进行中项"
    );
}

#[path = "mobile_unverified_task_progress_tests.rs"]
mod unverified_task_progress;
