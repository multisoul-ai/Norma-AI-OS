use canvas::{CanvasRect, EgoCardPhase};
use runtime::{EgoOwnership, EgoTaskCommand, EgoTaskError, Runtime};

/// Ego 新任务替换：只有结束任务可进入替换；旧精确 ID 必须保留到新空间确认成功，忙碌任务必须原样保留。
///
/// 数据构造（含关键数值的推导过程）：
///   Ego card count = 1（替换必须复用同一个 node）
///   old task_space_id = "old-8"，new display name = "Norma · Ego · 1"
///   completed Agent node_id = 10（Inactive，可替换）
///   busy Agent node_id = 20（AgentWorking，不可替换）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建并完成第一张 Ego 任务，再调用 replace。
///   2. 读取同一 node 的阶段、Task Space ID 和名称。
///   3. 在第二张正在工作的 Ego 卡片上调用 replace。
///   4. 检查忙碌错误与原 Task Space ID 未被破坏。
///
/// 预期结果：
///   - 正断言：完成态替换后复用原 node、进入 Creating，并保留旧 ID 直到 helper 成功确认新 ID。
///   - 正断言：TaskSpaceReady 到达后才以新 ID/名称原子覆盖旧标识。
///   - 正断言：忙碌替换返回 Busy 且指出 Agent 20。
///   - 负断言：忙碌任务不得丢失 old ID 或退化为 Creating。
#[test]
fn replace_ego_task_reuses_completed_card_and_rejects_busy_task() {
    let mut completed = Runtime::new();
    let completed_node = completed.spawn_ego_browser(
        Some("旧任务".to_string()),
        CanvasRect::new(0.0, 0.0, 960.0, 640.0),
    );
    completed
        .apply_ego_task_command(
            completed_node,
            EgoTaskCommand::TaskSpaceReady {
                task_space_id: "old-8".to_string(),
                task_space_name: "旧任务".to_string(),
            },
        )
        .expect("完成态 fixture 应先记录旧 Task Space");
    completed
        .apply_ego_task_command(
            completed_node,
            EgoTaskCommand::PrepareAgent { agent_node_id: 10 },
        )
        .expect("Agent 10 应能准备旧任务");
    completed
        .apply_ego_task_command(
            completed_node,
            EgoTaskCommand::AgentRunning { agent_node_id: 10 },
        )
        .expect("Agent 10 应能开始旧任务");
    completed
        .apply_ego_task_command(
            completed_node,
            EgoTaskCommand::Complete { agent_node_id: 10 },
        )
        .expect("Agent 10 应能完成旧任务");

    completed
        .replace_ego_task_space(completed_node)
        .expect("已完成任务应允许安全替换");
    let replaced_state = completed
        .ego_task_state(completed_node)
        .expect("替换后应继续保留同一 Ego 状态");
    let replaced_node = completed
        .document
        .node(completed_node)
        .expect("替换后应继续复用原画布节点");
    let canvas::NodeKind::Browser(canvas::BrowserSpec {
        surface: canvas::BrowserSurfaceKind::EgoLite(replaced_spec),
        ..
    }) = &replaced_node.kind
    else {
        panic!("替换后节点必须继续是 EgoLite Browser");
    };

    assert_eq!(
        replaced_state.phase,
        EgoCardPhase::Creating,
        "已完成任务替换后必须进入 Creating，等待 app 异步创建新空间"
    );
    assert_eq!(
        replaced_node.id, completed_node,
        "替换必须复用同一个画布节点，不能新建第二张 Ego 卡片"
    );
    assert_eq!(
        replaced_spec.task_space_id.as_deref(),
        Some("old-8"),
        "异步所有权确认完成前必须保留旧 Task Space ID，失败时才不会丢失可信标识"
    );
    assert_eq!(
        replaced_spec.task_space_name.as_deref(),
        Some("旧任务"),
        "新空间尚未确认前必须保留旧显示名称，避免 ID 与名称错配"
    );

    completed
        .apply_ego_task_command(
            completed_node,
            EgoTaskCommand::TaskSpaceReady {
                task_space_id: "new-1".to_string(),
                task_space_name: "Norma · Ego · 1".to_string(),
            },
        )
        .expect("helper 成功后应以新精确 ID 覆盖旧空间标识");
    let ready_node = completed
        .document
        .node(completed_node)
        .expect("新空间确认后原 Ego 卡片必须继续存在");
    let canvas::NodeKind::Browser(canvas::BrowserSpec {
        surface: canvas::BrowserSurfaceKind::EgoLite(ready_spec),
        ..
    }) = &ready_node.kind
    else {
        panic!("新空间确认后节点必须继续是 EgoLite Browser");
    };
    assert_eq!(
        ready_spec.task_space_id.as_deref(),
        Some("new-1"),
        "TaskSpaceReady 后必须用新精确 ID 原子覆盖旧 ID"
    );
    assert_ne!(
        ready_spec.task_space_id.as_deref(),
        Some("old-8"),
        "新空间确认后不得继续向 Agent 暴露旧 Task Space ID"
    );

    let mut busy = Runtime::new();
    let busy_node = busy.spawn_ego_browser(
        Some("忙碌任务".to_string()),
        CanvasRect::new(0.0, 0.0, 960.0, 640.0),
    );
    busy.apply_ego_task_command(
        busy_node,
        EgoTaskCommand::TaskSpaceReady {
            task_space_id: "busy-9".to_string(),
            task_space_name: "忙碌任务".to_string(),
        },
    )
    .expect("忙碌态 fixture 应先记录 Task Space");
    busy.apply_ego_task_command(
        busy_node,
        EgoTaskCommand::PrepareAgent { agent_node_id: 20 },
    )
    .expect("Agent 20 应能准备忙碌任务");
    busy.apply_ego_task_command(
        busy_node,
        EgoTaskCommand::AgentRunning { agent_node_id: 20 },
    )
    .expect("Agent 20 应能进入工作态");

    let busy_result = busy.replace_ego_task_space(busy_node);
    let busy_node_after = busy
        .document
        .node(busy_node)
        .expect("忙碌替换失败后节点必须仍存在");
    let canvas::NodeKind::Browser(canvas::BrowserSpec {
        surface: canvas::BrowserSurfaceKind::EgoLite(busy_spec),
        ..
    }) = &busy_node_after.kind
    else {
        panic!("忙碌替换失败后节点必须继续是 EgoLite Browser");
    };

    assert_eq!(
        busy_result,
        Err(EgoTaskError::Busy { current_owner: 20 }),
        "运行中的 Agent 20 任务必须拒绝替换并明确当前所有者"
    );
    assert_eq!(
        busy_spec.task_space_id.as_deref(),
        Some("busy-9"),
        "忙碌替换失败后必须保留原 Task Space ID"
    );
    assert_eq!(
        busy.ego_task_state(busy_node).map(|state| state.phase),
        Some(EgoCardPhase::AgentWorking),
        "忙碌替换失败后不得把任务错误变成 Creating"
    );
}

/// Ego 新任务终态边界：`--new` 的底层替换只接受 Completed 或 Failed，不能把尚未开始的空间覆盖掉。
///
/// 数据构造（含关键数值的推导过程）：
///   Ego card count = 1（替换失败也必须保持单实例）
///   ready task_space_id = "ready-3"，名称 = "等待中的任务"
///   ReadyForUser = 已创建但尚未由 Agent 完成，不属于终态集合 {Completed, Failed}
///   replace attempts = 1，因此预期清空 ID 次数 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Ego 卡片并写入已就绪的精确 Task Space ID。
///   2. 在 ReadyForUser 阶段请求替换任务空间。
///   3. 读取错误、阶段和持久化 ID，确认状态机没有提前改写卡片。
///
/// 预期结果：
///   - 正断言：替换返回 ReadyForUser 的 InvalidTransition，调用方可据此拒绝 `present ego --new`。
///   - 正断言：阶段和精确 ID 均保持原值，用户仍可进入原空间。
///   - 负断言：不得进入 Creating；不得清空 "ready-3" 或创建第二张卡。
#[test]
fn replace_ego_task_rejects_ready_for_user_even_though_card_may_close() {
    let mut runtime = Runtime::new();
    let node_id = runtime.spawn_ego_browser(
        Some("等待中的任务".to_string()),
        CanvasRect::new(0.0, 0.0, 960.0, 640.0),
    );
    runtime
        .apply_ego_task_command(
            node_id,
            EgoTaskCommand::TaskSpaceReady {
                task_space_id: "ready-3".to_string(),
                task_space_name: "等待中的任务".to_string(),
            },
        )
        .expect("fixture 应先把 Ego 卡片推进到 ReadyForUser");

    let replacement = runtime.replace_ego_task_space(node_id);
    let node = runtime
        .document
        .node(node_id)
        .expect("替换被拒绝后原 Ego 画布节点必须继续存在");
    let canvas::NodeKind::Browser(canvas::BrowserSpec {
        surface: canvas::BrowserSurfaceKind::EgoLite(spec),
        ..
    }) = &node.kind
    else {
        panic!("替换被拒绝后节点必须继续是 EgoLite Browser");
    };

    assert_eq!(
        replacement,
        Err(EgoTaskError::InvalidTransition {
            phase: EgoCardPhase::ReadyForUser,
            operation: "replace_task_space",
        }),
        "ReadyForUser 不是终态，`--new` 替换必须被拒绝"
    );
    assert_eq!(
        runtime.ego_task_state(node_id).map(|state| state.phase),
        Some(EgoCardPhase::ReadyForUser),
        "替换失败后卡片必须仍停留在 ReadyForUser"
    );
    assert_eq!(
        spec.task_space_id.as_deref(),
        Some("ready-3"),
        "替换失败不得清空用户尚可打开的精确 Task Space ID"
    );
    assert_ne!(
        runtime.ego_task_state(node_id).map(|state| state.phase),
        Some(EgoCardPhase::Creating),
        "ReadyForUser 的拒绝路径不得把卡片错误推进到 Creating"
    );
    assert_eq!(
        runtime.ego_node_id(),
        Some(node_id),
        "拒绝替换不得创建第二张 Ego 卡片或丢失原单实例"
    );
}

/// Ego 安全替换失败：旧空间仍存在但所有权查询拒绝时，卡片必须保留精确 ID 并进入可见断开态。
///
/// 数据构造（含关键数值的推导过程）：
///   terminal fixture = ReadyForUser → PreparingAgent → AgentWorking → Completed，共 4 次状态推进
///   old task_space_id = "protected-6"，原绑定 Agent node_id = 10
///   replacement attempts = 1；helper close success count = 0（ownership 查询先拒绝）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建并完成 Agent 10 的旧任务，使其满足 `--new` 的终态前置条件。
///   2. 进入替换准备态，但不让 helper 关闭旧空间。
///   3. 报告“旧空间仍存在”的安全失败，读取卡片状态和持久化 spec。
///
/// 预期结果：
///   - 正断言：卡片进入 Disconnected + Unknown，明确显示失败而非假装新空间已创建。
///   - 正断言：精确 ID/名称与原绑定 Agent 全部保留，用户仍能识别和手动打开旧空间。
///   - 负断言：安全失败不得清空 "protected-6"，也不得把所有权转给新任务。
#[test]
fn failed_safe_replacement_preserves_existing_task_space_identity() {
    let mut runtime = Runtime::new();
    let node_id = runtime.spawn_ego_browser(
        Some("受保护的旧任务".to_string()),
        CanvasRect::new(0.0, 0.0, 960.0, 640.0),
    );
    runtime
        .apply_ego_task_command(
            node_id,
            EgoTaskCommand::TaskSpaceReady {
                task_space_id: "protected-6".to_string(),
                task_space_name: "受保护的旧任务".to_string(),
            },
        )
        .expect("fixture 应先创建旧 Task Space");
    runtime
        .apply_ego_task_command(node_id, EgoTaskCommand::PrepareAgent { agent_node_id: 10 })
        .expect("fixture 应先绑定 Agent 10");
    runtime
        .apply_ego_task_command(node_id, EgoTaskCommand::AgentRunning { agent_node_id: 10 })
        .expect("fixture 应让 Agent 10 开始旧任务");
    runtime
        .apply_ego_task_command(node_id, EgoTaskCommand::Complete { agent_node_id: 10 })
        .expect("fixture 应完成旧任务以允许替换");
    runtime
        .replace_ego_task_space(node_id)
        .expect("Completed 旧任务应能进入安全替换准备态");

    runtime
        .fail_ego_task_space_replacement(
            node_id,
            true,
            "不会关闭 user 所有的 Task Space".to_string(),
        )
        .expect("所有权查询拒绝后应能记录安全替换失败");
    let state = runtime
        .ego_task_state(node_id)
        .expect("安全失败后 Ego 状态必须仍保留供 UI 展示");
    let node = runtime
        .document
        .node(node_id)
        .expect("安全失败后原 Ego 画布节点必须继续存在");
    let canvas::NodeKind::Browser(canvas::BrowserSpec {
        surface: canvas::BrowserSurfaceKind::EgoLite(spec),
        ..
    }) = &node.kind
    else {
        panic!("安全失败后节点必须继续是 EgoLite Browser");
    };

    assert_eq!(
        state.phase,
        EgoCardPhase::Disconnected,
        "所有权查询失败后卡片必须进入可见 Disconnected 状态"
    );
    assert_eq!(
        state.ownership,
        EgoOwnership::Unknown,
        "未重新查询旧空间时所有权必须回到 Unknown，而不是猜测仍由 Agent 控制"
    );
    assert_eq!(
        state.bound_agent_node_id,
        Some(10),
        "旧空间仍存在时安全失败必须保留原绑定 Agent 10"
    );
    assert_eq!(
        spec.task_space_id.as_deref(),
        Some("protected-6"),
        "安全失败不得清空仍存在的精确 Task Space ID"
    );
    assert_eq!(
        spec.task_space_name.as_deref(),
        Some("受保护的旧任务"),
        "安全失败不得把旧 ID 错配为尚未创建的新任务名称"
    );
    assert_ne!(
        spec.task_space_id, None,
        "仍存在的旧空间在安全失败后不得丢失唯一可信标识"
    );
}
