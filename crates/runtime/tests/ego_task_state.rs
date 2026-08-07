use canvas::{CanvasRect, EgoCardPhase};
use runtime::{EgoOwnership, EgoTaskCommand, EgoTaskError, Runtime};

/// Ego 交接闭环：单 Agent 所有权、显式 handoff、用户交还和原 Agent 恢复必须受状态机守卫。
///
/// 数据构造（含关键数值的推导过程）：
///   Ego card count = 1
///   original agent node_id = 10
///   competing agent node_id = 11
///   bounds = 960×640，面积 = 614_400 canvas px²，仅用于创建画布节点
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Ego 卡片并绑定 Task Space "42"。
///   2. Agent 10 准备并报告 running。
///   3. Agent 11 尝试接管，系统拒绝且保留 Agent 10。
///   4. 用户在 handoff 前尝试接管，系统拒绝。
///   5. Agent 10 handoff，用户接管并点击交还。
///   6. Agent 10 显式报告 running，随后完成任务。
///
/// 预期结果：
///   - 正断言：阶段依次到达 ReadyForUser、AgentWorking、AwaitingUser、
///     UserControlling、ResumeRequested、AgentWorking、Completed。
///   - 正断言：冲突错误明确携带 Agent 10，完成后允许关闭。
///   - 负断言：Agent 11 不得取得所有权，handoff 前用户不得接管。
#[test]
fn ego_task_handoff_requires_bound_agent_and_explicit_resume() {
    let mut runtime = Runtime::new();
    let node_id = runtime.spawn_ego_browser(
        Some("检查发布结果".to_string()),
        CanvasRect::new(0.0, 0.0, 960.0, 640.0),
    );

    runtime
        .apply_ego_task_command(
            node_id,
            EgoTaskCommand::TaskSpaceReady {
                task_space_id: "42".to_string(),
                task_space_name: "检查发布结果".to_string(),
            },
        )
        .expect("Task Space ready 应使卡片可供用户操作");
    assert_eq!(
        runtime.ego_task_state(node_id).map(|state| state.phase),
        Some(EgoCardPhase::ReadyForUser),
        "Task Space ready 后阶段必须是 ReadyForUser"
    );

    runtime
        .apply_ego_task_command(node_id, EgoTaskCommand::PrepareAgent { agent_node_id: 10 })
        .expect("空闲 Ego 卡片应允许 Agent 10 准备接管");
    runtime
        .apply_ego_task_command(node_id, EgoTaskCommand::AgentRunning { agent_node_id: 10 })
        .expect("绑定 Agent 10 报告 running 后应进入工作状态");

    let competing =
        runtime.apply_ego_task_command(node_id, EgoTaskCommand::PrepareAgent { agent_node_id: 11 });
    assert_eq!(
        competing,
        Err(EgoTaskError::Busy { current_owner: 10 }),
        "第二个 Agent 必须被拒绝，并明确返回当前所有者 Agent 10"
    );
    assert_eq!(
        runtime.ego_task_state(node_id).map(|state| state.ownership),
        Some(EgoOwnership::Agent { node_id: 10 }),
        "冲突请求后所有权必须仍属于 Agent 10"
    );
    assert_ne!(
        runtime.ego_task_state(node_id).map(|state| state.ownership),
        Some(EgoOwnership::Agent { node_id: 11 }),
        "Agent 11 不得通过冲突请求取得 Ego 所有权"
    );

    let early_takeover = runtime.apply_ego_task_command(node_id, EgoTaskCommand::UserTakeover);
    assert_eq!(
        early_takeover,
        Err(EgoTaskError::InvalidTransition {
            phase: EgoCardPhase::AgentWorking,
            operation: "user_takeover",
        }),
        "Agent 未主动 handoff 前，用户接管必须被状态机拒绝"
    );

    runtime
        .apply_ego_task_command(
            node_id,
            EgoTaskCommand::AgentHandoff {
                agent_node_id: 10,
                message: "请完成登录".to_string(),
            },
        )
        .expect("绑定 Agent 10 应能主动 handoff");
    assert_eq!(
        runtime.ego_task_state(node_id).map(|state| state.phase),
        Some(EgoCardPhase::AwaitingUser),
        "Agent handoff 后必须进入 AwaitingUser"
    );

    runtime
        .apply_ego_task_command(node_id, EgoTaskCommand::UserTakeover)
        .expect("AwaitingUser 阶段应允许用户接管");
    assert_eq!(
        runtime.ego_task_state(node_id).map(|state| state.phase),
        Some(EgoCardPhase::UserControlling),
        "用户接管后必须进入 UserControlling"
    );
    assert!(
        !runtime.can_close_ego_node(node_id),
        "用户控制中的 Ego 卡片属于忙碌状态，不得关闭"
    );

    runtime
        .apply_ego_task_command(node_id, EgoTaskCommand::UserReturn)
        .expect("用户控制中应能点击交还 AI");
    assert_eq!(
        runtime.ego_task_state(node_id).map(|state| state.phase),
        Some(EgoCardPhase::ResumeRequested),
        "交还 AI 只应进入 ResumeRequested，不能直接恢复 AgentWorking"
    );
    assert_ne!(
        runtime.ego_task_state(node_id).map(|state| state.phase),
        Some(EgoCardPhase::AgentWorking),
        "Norma 不得在用户交还时替 Agent 自动接管"
    );

    runtime
        .apply_ego_task_command(node_id, EgoTaskCommand::AgentRunning { agent_node_id: 10 })
        .expect("原 Agent 10 显式接管后应恢复工作");
    runtime
        .apply_ego_task_command(node_id, EgoTaskCommand::Complete { agent_node_id: 10 })
        .expect("绑定 Agent 10 应能完成任务");

    let completed = runtime
        .ego_task_state(node_id)
        .expect("完成后 Ego 状态仍应保留供卡片展示");
    assert_eq!(
        completed.phase,
        EgoCardPhase::Completed,
        "完成命令后阶段必须为 Completed"
    );
    assert_eq!(
        completed.ownership,
        EgoOwnership::Inactive,
        "完成后所有权必须归为 Inactive"
    );
    assert!(
        runtime.can_close_ego_node(node_id),
        "已完成 Ego 卡片必须允许用户关闭"
    );
}

/// Ego 恢复：Task Space 必须按精确 ID 持久化，重载后进入 Unknown/Disconnected 安全状态。
///
/// 数据构造（含关键数值的推导过程）：
///   Task Space ID = "314"，名称 = "恢复发布检查"
///   snapshot node count = 1
///   外部 Ego 查询次数 = 0（runtime 是纯域层，不在恢复时触发副作用）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Ego 卡片并应用 TaskSpaceReady。
///   2. 收集 DocumentSnapshot。
///   3. 新 Runtime 从 snapshot 恢复。
///   4. 读取恢复后的 Ego 状态和画布持久化 spec。
///
/// 预期结果：
///   - 正断言：精确 ID "314" 与名称完整保留。
///   - 正断言：恢复状态为 Disconnected + Unknown，等待 app 可见时重连。
///   - 负断言：恢复不得猜测为 AgentWorking 或丢失 Task Space ID。
#[test]
fn restored_ego_task_preserves_exact_id_and_requires_reconnect() {
    let mut original = Runtime::new();
    let node_id = original.spawn_ego_browser(
        Some("恢复发布检查".to_string()),
        CanvasRect::new(0.0, 0.0, 960.0, 640.0),
    );
    original
        .apply_ego_task_command(
            node_id,
            EgoTaskCommand::TaskSpaceReady {
                task_space_id: "314".to_string(),
                task_space_name: "恢复发布检查".to_string(),
            },
        )
        .expect("TaskSpaceReady 应持久化精确 ID");
    let snapshot = original.collect_canvas_snapshot();

    let mut restored = Runtime::new();
    restored.restore_canvas_snapshot(snapshot);

    let state = restored
        .ego_task_state(node_id)
        .expect("恢复后必须重建 Ego runtime 状态");
    assert_eq!(
        state.phase,
        EgoCardPhase::Disconnected,
        "恢复后必须进入 Disconnected，等待可见时精确重连"
    );
    assert_eq!(
        state.ownership,
        EgoOwnership::Unknown,
        "恢复前未查询 Ego，所有权必须为 Unknown"
    );
    assert_ne!(
        state.phase,
        EgoCardPhase::AgentWorking,
        "仅凭持久化 last_phase 不得猜测 Agent 仍在工作"
    );
    let node = restored
        .document
        .node(node_id)
        .expect("恢复后 Ego 画布节点必须存在");
    let canvas::NodeKind::Browser(canvas::BrowserSpec {
        surface: canvas::BrowserSurfaceKind::EgoLite(spec),
        ..
    }) = &node.kind
    else {
        panic!("恢复后的节点必须仍是 EgoLite Browser");
    };
    assert_eq!(
        spec.task_space_id.as_deref(),
        Some("314"),
        "恢复必须使用精确 Task Space ID，不得丢失或按名称猜测"
    );
    assert_eq!(
        spec.task_space_name.as_deref(),
        Some("恢复发布检查"),
        "Task Space 显示名称必须完整保留"
    );
}

/// Ego 恢复所有权：持久化过精确 Task Space ID 的卡片恢复为 Unknown 后，竞争 Agent 不能借此重绑。
///
/// 数据构造（含关键数值的推导过程）：
///   original Agent node_id = 10；competing Agent node_id = 11
///   Task Space ID = "restore-owned-5"，snapshot node count = 1
///   恢复后 phase = Disconnected、ownership = Unknown，但 bound_agent_node_id 仍为 10
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Agent 10 创建、绑定并准备拥有 Task Space，然后把画布快照交给新 Runtime。
///   2. 新 Runtime 按安全恢复规则进入 Disconnected + Unknown。
///   3. Agent 11 尝试 PrepareAgent，随后读取绑定和阶段。
///   4. 原 Agent 10 再尝试 PrepareAgent，确认它可以显式恢复工作流。
///
/// 预期结果：
///   - 正断言：Agent 11 收到 Unauthorized，卡片仍绑定 Agent 10 且保持 Disconnected。
///   - 正断言：原 Agent 10 可以进入 PreparingAgent，重新取得已知空间的工作权。
///   - 负断言：恢复安全态不得把旧 Task Space 转交给 Agent 11。
#[test]
fn restored_ego_task_rejects_competing_agent_rebind() {
    let mut original = Runtime::new();
    let node_id = original.spawn_ego_browser(
        Some("恢复所有权检查".to_string()),
        CanvasRect::new(0.0, 0.0, 960.0, 640.0),
    );
    original
        .apply_ego_task_command(
            node_id,
            EgoTaskCommand::TaskSpaceReady {
                task_space_id: "restore-owned-5".to_string(),
                task_space_name: "恢复所有权检查".to_string(),
            },
        )
        .expect("fixture 应先记录精确 Task Space ID");
    original
        .apply_ego_task_command(node_id, EgoTaskCommand::PrepareAgent { agent_node_id: 10 })
        .expect("fixture 应先把 Task Space 绑定给原 Agent 10");
    let snapshot = original.collect_canvas_snapshot();

    let mut restored = Runtime::new();
    restored.restore_canvas_snapshot(snapshot);
    let competing = restored
        .apply_ego_task_command(node_id, EgoTaskCommand::PrepareAgent { agent_node_id: 11 });

    assert_eq!(
        competing,
        Err(EgoTaskError::Unauthorized {
            expected_agent: 10,
            actual_agent: 11,
        }),
        "恢复后的已绑定 Task Space 必须拒绝竞争 Agent 11 的重绑"
    );
    assert_eq!(
        restored.ego_task_state(node_id).map(|state| state.phase),
        Some(EgoCardPhase::Disconnected),
        "竞争 Agent 被拒绝后恢复安全态必须保持 Disconnected"
    );
    assert_eq!(
        restored
            .ego_task_state(node_id)
            .and_then(|state| state.bound_agent_node_id),
        Some(10),
        "竞争 Agent 被拒绝后卡片必须仍绑定原 Agent 10"
    );
    assert_ne!(
        restored
            .ego_task_state(node_id)
            .and_then(|state| state.bound_agent_node_id),
        Some(11),
        "恢复安全态不得把旧 Task Space 转交给 Agent 11"
    );

    restored
        .apply_ego_task_command(node_id, EgoTaskCommand::PrepareAgent { agent_node_id: 10 })
        .expect("原 Agent 10 应能显式恢复已绑定 Task Space 的工作流");
    assert_eq!(
        restored.ego_task_state(node_id).map(|state| state.phase),
        Some(EgoCardPhase::PreparingAgent),
        "原 Agent 10 重新准备后必须进入 PreparingAgent"
    );
}
