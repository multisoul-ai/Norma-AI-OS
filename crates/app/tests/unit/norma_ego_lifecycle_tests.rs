use super::*;
use canvas::EgoCardPhase;
use norma::protocol::{
    EgoCaptureMode, EgoCommand, EgoErrorCode, EgoPhase, EgoPresentationDisposition,
};

/// Ego 展示：首次创建后绑定来源 Agent，但保持普通画布布局；同 Agent 复用，第二 Agent 被拒绝。
///
/// 数据构造（含关键数值的推导过程）：
///   可见区域 = 1200×800，zoom=1.0
///   source Agent A node_id=10，竞争 Agent B node_id=20
///   首次请求 + 同 Agent 复用 + 第二 Agent 请求 = 3 次展示调用
///   单实例上限=1 张 Ego 卡片
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Agent A `present ego` → 创建 Ego 节点并绑定 A
///   2. Agent A 再次 `present ego` → 复用同一节点
///   3. Agent B `present ego` → Runtime 忙碌守卫拒绝
///   4. 检查 Master 布局始终未开启
///
/// 预期结果：
///   - 正断言：首次 disposition=created、阶段=preparing_agent、capture=external_window
///   - 正断言：第二次 disposition=reused 且 node_id 不变
///   - 正断言：Agent B 收到结构化 busy 错误
///   - 负断言：不得创建第二张卡片，也不得自动进入 Master
#[test]
fn test_present_ego_is_single_non_master_card_with_agent_busy_guard() {
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(false);
    let master_layout = crate::canvas::master_layout::MasterLayoutState::default();
    let area = CanvasRect::new(0.0, 0.0, 1200.0, 800.0);

    let created = presentation::execute_ego_presentation_for_area(
        &mut runtime,
        &mut motion,
        &master_layout,
        false,
        10,
        area,
        1.0,
    )
    .expect("Agent A 首次 present ego 应成功");
    let reused = presentation::execute_ego_presentation_for_area(
        &mut runtime,
        &mut motion,
        &master_layout,
        false,
        10,
        area,
        1.0,
    )
    .expect("同一 Agent 再次 present ego 应复用");
    let busy = presentation::execute_ego_presentation_for_area(
        &mut runtime,
        &mut motion,
        &master_layout,
        false,
        20,
        area,
        1.0,
    )
    .expect_err("第二 Agent 不得接管忙碌 Ego 卡片");

    let (created_status, created_disposition) = match created.result {
        CardCommandResult::EgoPresented {
            status,
            disposition,
        } => (status, disposition),
        other => panic!("首次 Ego 展示应返回 EgoPresented；实际：{other:?}"),
    };
    let (reused_status, reused_disposition) = match reused.result {
        CardCommandResult::EgoPresented {
            status,
            disposition,
        } => (status, disposition),
        other => panic!("复用 Ego 展示应返回 EgoPresented；实际：{other:?}"),
    };

    assert_eq!(
        created_disposition,
        EgoPresentationDisposition::Created,
        "首次 Ego 展示必须报告 created"
    );
    assert_eq!(
        created_status.phase,
        EgoPhase::PreparingAgent,
        "首次 Agent 展示后必须进入 preparing_agent"
    );
    assert_eq!(
        created_status.capture_mode,
        EgoCaptureMode::ExternalWindow,
        "POC 闸门失败后必须明确使用 external_window"
    );
    assert!(!created_status.master, "Ego 创建后不得自动成为 Master");
    assert_eq!(
        reused_disposition,
        EgoPresentationDisposition::Reused,
        "同 Agent 再次展示必须复用现有 Ego 卡片"
    );
    assert_eq!(
        reused_status.node_id, created_status.node_id,
        "复用请求必须返回同一个 Ego node_id"
    );
    assert!(
        matches!(
            busy,
            CardCommandError::Ego(ref failure) if failure.code == EgoErrorCode::Busy
        ),
        "第二 Agent 必须收到结构化 busy 错误；实际：{busy:?}"
    );
    assert_eq!(
        runtime.document.nodes().len(),
        1,
        "三次展示调用后仍只能存在一张 Ego 卡片"
    );
    assert!(
        !master_layout.is_active(),
        "Ego 展示不得自动开启 Master 布局"
    );
}

/// Ego CLI 生命周期：只有绑定 Agent 可报告工作、交接、完成，并能查询精确 Task Space。
///
/// 数据构造（含关键数值的推导过程）：
///   Ego node_id 由 Runtime 分配，绑定 Agent node_id=10
///   Task Space ID="space-7"，名称="Norma · Ego"
///   生命周期命令数=4：running → handoff → status → complete(非法阶段)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Ego、绑定 Agent 10，并记录 Task Space 精确 ID
///   2. Agent 10 报告 running，再 handoff 给用户
///   3. 查询状态并检查 AwaitingUser/所有权
///   4. Agent 20 尝试 complete，验证授权先失败
///
/// 预期结果：
///   - 正断言：running 进入 agent_working，handoff 进入 awaiting_user
///   - 正断言：status 返回精确 task_space_id 与外部窗口模式
///   - 正断言：非绑定 Agent 收到 unauthorized
///   - 负断言：handoff 后不应误报 agent_working 或 Master
#[test]
fn test_ego_agent_commands_preserve_task_id_and_authorization() {
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(false);
    let master_layout = crate::canvas::master_layout::MasterLayoutState::default();
    let area = CanvasRect::new(0.0, 0.0, 1200.0, 800.0);
    let created = presentation::execute_ego_presentation_for_area(
        &mut runtime,
        &mut motion,
        &master_layout,
        false,
        10,
        area,
        1.0,
    )
    .expect("Ego 展示应创建并绑定 Agent 10");
    let node_id = match created.result {
        CardCommandResult::EgoPresented { status, .. } => status.node_id,
        other => panic!("Ego 展示应返回 EgoPresented；实际：{other:?}"),
    };
    runtime
        .apply_ego_task_command(
            node_id,
            runtime::EgoTaskCommand::TaskSpaceReady {
                task_space_id: "space-7".to_string(),
                task_space_name: "Norma · Ego".to_string(),
            },
        )
        .expect("后台创建 Task Space 后应记录精确 ID");

    let running = presentation::execute_ego_command_for_area(
        &mut runtime,
        &master_layout,
        node_id,
        EgoCommand::Running,
        10,
    )
    .expect("绑定 Agent 应可报告 running");
    let handoff = presentation::execute_ego_command_for_area(
        &mut runtime,
        &master_layout,
        node_id,
        EgoCommand::Handoff {
            message: "请完成登录".to_string(),
        },
        10,
    )
    .expect("绑定 Agent 应可 handoff");
    let status = presentation::execute_ego_command_for_area(
        &mut runtime,
        &master_layout,
        node_id,
        EgoCommand::Status,
        10,
    )
    .expect("绑定 Agent 应可查询 Ego 状态");
    let unauthorized = presentation::execute_ego_command_for_area(
        &mut runtime,
        &master_layout,
        node_id,
        EgoCommand::Complete,
        20,
    )
    .expect_err("非绑定 Agent 不得完成 Ego 任务");

    let running_status = match running.result {
        CardCommandResult::EgoStatus { status } => status,
        other => panic!("running 应返回 EgoStatus；实际：{other:?}"),
    };
    let handoff_status = match handoff.result {
        CardCommandResult::EgoStatus { status } => status,
        other => panic!("handoff 应返回 EgoStatus；实际：{other:?}"),
    };
    let queried_status = match status.result {
        CardCommandResult::EgoStatus { status } => status,
        other => panic!("status 应返回 EgoStatus；实际：{other:?}"),
    };

    assert_eq!(
        running_status.phase,
        EgoPhase::AgentWorking,
        "running 必须进入 agent_working"
    );
    assert_eq!(
        handoff_status.phase,
        EgoPhase::AwaitingUser,
        "handoff 必须进入 awaiting_user"
    );
    assert_ne!(
        handoff_status.phase,
        EgoPhase::AgentWorking,
        "handoff 后不得继续误报 agent_working"
    );
    assert_eq!(
        queried_status.task_space_id.as_deref(),
        Some("space-7"),
        "status 必须返回精确 Task Space ID"
    );
    assert_eq!(
        queried_status.capture_mode,
        EgoCaptureMode::ExternalWindow,
        "status 必须公开外部窗口降级模式"
    );
    assert!(!queried_status.master, "Ego 状态不得误报为 Master");
    assert!(
        matches!(
            unauthorized,
            CardCommandError::Ego(ref failure)
                if failure.code == EgoErrorCode::Unauthorized
        ),
        "非绑定 Agent 必须收到 unauthorized；实际：{unauthorized:?}"
    );
}

/// Ego --new：已完成任务必须复用卡片、对协议隐藏旧 ID 但持久化保留它；运行中任务必须返回 Busy 且不改变卡片。
///
/// 数据构造（含关键数值的推导过程）：
///   可见区域 = 1200×800，zoom=1.0
///   绑定 Agent A node_id=10
///   old Task Space ID = "old-7"
///   调用数 = 2：已完成 --new + 运行中 --new
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Ego、记录 old-7、让 Agent A 工作并完成。
///   2. 以 create_new=true 再次展示，分别读取协议状态和持久化 ID。
///   3. 让新任务重新进入 AgentWorking。
///   4. 再次以 create_new=true 请求，检查 Busy 错误和原 node。
///
/// 预期结果：
///   - 正断言：完成态返回 replaced、复用 node，协议暂不暴露 old-7。
///   - 正断言：后台所有权确认未完成前，持久化仍保留 old-7，失败时不会丢失可信标识。
///   - 正断言：替换后进入 preparing_agent，等待异步新 Task Space。
///   - 正断言：运行中请求返回 busy。
///   - 负断言：运行中请求不得创建第二张卡片或清空当前状态。
#[test]
fn test_present_ego_new_replaces_completed_task_but_rejects_busy_task() {
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(false);
    let master_layout = crate::canvas::master_layout::MasterLayoutState::default();
    let area = CanvasRect::new(0.0, 0.0, 1200.0, 800.0);
    let created = presentation::execute_ego_presentation_for_area(
        &mut runtime,
        &mut motion,
        &master_layout,
        false,
        10,
        area,
        1.0,
    )
    .expect("首次 Ego 展示应成功");
    let node_id = match created.result {
        CardCommandResult::EgoPresented { status, .. } => status.node_id,
        other => panic!("首次 Ego 展示应返回 EgoPresented；实际：{other:?}"),
    };
    runtime
        .apply_ego_task_command(
            node_id,
            runtime::EgoTaskCommand::TaskSpaceReady {
                task_space_id: "old-7".to_string(),
                task_space_name: "Norma · Ego · 1".to_string(),
            },
        )
        .expect("fixture 应记录旧 Task Space");
    presentation::execute_ego_command_for_area(
        &mut runtime,
        &master_layout,
        node_id,
        EgoCommand::Running,
        10,
    )
    .expect("绑定 Agent 应能报告旧任务 running");
    presentation::execute_ego_command_for_area(
        &mut runtime,
        &master_layout,
        node_id,
        EgoCommand::Complete,
        10,
    )
    .expect("绑定 Agent 应能完成旧任务");

    let replaced = presentation::execute_ego_presentation_for_area(
        &mut runtime,
        &mut motion,
        &master_layout,
        true,
        10,
        area,
        1.0,
    )
    .expect("已完成 Ego 任务应允许 --new");
    let (replaced_status, disposition) = match replaced.result {
        CardCommandResult::EgoPresented {
            status,
            disposition,
        } => (status, disposition),
        other => panic!("--new 应返回 EgoPresented；实际：{other:?}"),
    };
    let persisted_old_task_space_id =
        runtime
            .document
            .node(node_id)
            .and_then(|node| match &node.kind {
                canvas::CanvasNodeKind::Browser(spec) => match &spec.surface {
                    canvas::BrowserSurfaceKind::EgoLite(ego) => ego.task_space_id.clone(),
                    canvas::BrowserSurfaceKind::Native => None,
                },
                _ => None,
            });

    assert_eq!(
        disposition,
        EgoPresentationDisposition::Replaced,
        "已完成任务的 --new 必须报告 replaced"
    );
    assert_eq!(
        replaced_status.node_id, node_id,
        "--new 必须复用原 Ego 卡片，而不是创建第二张"
    );
    assert_eq!(
        replaced_status.task_space_id, None,
        "--new 的协议状态必须暂不暴露 old-7，避免 Agent 误用正在替换的旧空间"
    );
    assert_eq!(
        persisted_old_task_space_id.as_deref(),
        Some("old-7"),
        "后台所有权确认未完成前持久化必须保留 old-7，安全失败时不会丢失可信标识"
    );
    assert_eq!(
        replaced_status.phase,
        EgoPhase::PreparingAgent,
        "替换后绑定 Agent A 的卡片必须进入 preparing_agent"
    );

    presentation::execute_ego_command_for_area(
        &mut runtime,
        &master_layout,
        node_id,
        EgoCommand::Running,
        10,
    )
    .expect("新任务的 Agent A 应能报告 running");
    let busy = presentation::execute_ego_presentation_for_area(
        &mut runtime,
        &mut motion,
        &master_layout,
        true,
        10,
        area,
        1.0,
    )
    .expect_err("运行中的 Ego 任务不得允许 --new");

    assert!(
        matches!(
            busy,
            CardCommandError::Ego(ref failure) if failure.code == EgoErrorCode::Busy
        ),
        "运行中的 --new 必须返回 structured busy；实际：{busy:?}"
    );
    assert_eq!(
        runtime.document.nodes().len(),
        1,
        "运行中的 --new 失败后不得创建第二张 Ego 卡片"
    );
    assert_eq!(
        runtime.ego_task_state(node_id).map(|state| state.phase),
        Some(EgoCardPhase::AgentWorking),
        "运行中的 --new 失败后当前任务必须继续保持 AgentWorking"
    );
}
