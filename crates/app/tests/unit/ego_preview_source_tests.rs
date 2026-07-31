use super::*;
use runtime::EgoTaskCommand;
use std::collections::{HashMap, HashSet};

/// Ego 持续预览源选择：只有 Agent 已确认运行时才使用 CDP，其余阶段必须使用可供用户观察的窗口流。
///
/// 数据构造（含关键数值的推导过程）：
///   Ego 节点 = 1（node_id 由 Runtime 分配）
///   状态采样 = ReadyForUser + AgentWorking + AwaitingUser = 3 次
///   CDP 允许次数 = AgentWorking(1)；CDP 禁止次数 = 3 - 1 = 2
///   Task Space ID = "source-policy-7"，Task Space 名称 = "Ego source policy"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. TaskSpaceReady 后收集预览目标 → 用户可操作但 Agent 未持有，CDP 目标必须为空。
///   2. PrepareAgent + AgentRunning 后再次收集 → Agent 已持有且运行，CDP 必须包含该节点。
///   3. AgentHandoff 后再次收集 → 控制权已交给用户，CDP 必须立即退出目标集合。
///
/// 预期结果：
///   - 正断言：AgentWorking 时唯一 Ego 节点进入 CDP 24 FPS 主源。
///   - 正断言：ReadyForUser 与 AwaitingUser 时节点保留在窗口实时捕获候选中。
///   - 负断言：用户可操作或已交接阶段不得继续启动会被 Ego 所有权策略拒绝的 CDP 命令。
#[test]
fn test_cdp_capture_targets_require_confirmed_agent_ownership() {
    let mut runtime = runtime::Runtime::new();
    let node_id = runtime.spawn_ego_browser(
        Some("Ego source policy".to_string()),
        canvas::CanvasRect::new(0.0, 0.0, 960.0, 640.0),
    );
    runtime
        .apply_ego_task_command(
            node_id,
            EgoTaskCommand::TaskSpaceReady {
                task_space_id: "source-policy-7".to_string(),
                task_space_name: "Ego source policy".to_string(),
            },
        )
        .expect("测试必须先把 Ego 卡片推进到 ReadyForUser");

    let ready_targets = eligible_ego_preview_targets(&runtime);
    let ready_cdp = cdp_capture_targets(&runtime, &ready_targets);
    let ready_window = secondary_capture_targets(&ready_targets, |_| false);

    assert!(
        ready_cdp.is_empty(),
        "ReadyForUser 的 Task Space 可由用户操作，后台不得启动 CDP 抢占式命令"
    );
    assert_eq!(
        ready_window, ready_targets,
        "ReadyForUser 必须保留窗口实时捕获候选，不能因禁用 CDP 而失去预览"
    );

    runtime
        .apply_ego_task_command(node_id, EgoTaskCommand::PrepareAgent { agent_node_id: 73 })
        .expect("测试 Agent 73 必须能准备接管空闲 Ego 卡片");
    runtime
        .apply_ego_task_command(node_id, EgoTaskCommand::AgentRunning { agent_node_id: 73 })
        .expect("测试 Agent 73 确认接管后必须进入 AgentWorking");
    let working_targets = eligible_ego_preview_targets(&runtime);
    let working_cdp = cdp_capture_targets(&runtime, &working_targets);

    assert_eq!(
        working_cdp, working_targets,
        "AgentWorking 且所有权属于 Agent 73 时必须启用 CDP 24 FPS 主源"
    );
    assert!(
        working_cdp
            .iter()
            .any(|(candidate, _, _)| *candidate == node_id),
        "AgentWorking 的唯一 Ego 节点必须明确存在于 CDP 目标集合"
    );

    runtime
        .apply_ego_task_command(
            node_id,
            EgoTaskCommand::AgentHandoff {
                agent_node_id: 73,
                message: "请用户继续操作".to_string(),
            },
        )
        .expect("测试 Agent 73 必须能把 Ego 显式交接给用户");
    let handed_off_targets = eligible_ego_preview_targets(&runtime);
    let handed_off_cdp = cdp_capture_targets(&runtime, &handed_off_targets);
    let handed_off_window = secondary_capture_targets(&handed_off_targets, |_| false);

    assert!(
        handed_off_cdp.is_empty(),
        "AwaitingUser 已交接给用户后不得继续运行 CDP 浏览器命令"
    );
    assert_eq!(
        handed_off_window, handed_off_targets,
        "AwaitingUser 必须立即切到窗口实时捕获候选，尽量保持 24 FPS 预览"
    );
    assert!(
        !handed_off_cdp
            .iter()
            .any(|(candidate, _, _)| *candidate == node_id),
        "用户接管候选阶段不得残留 node_id 对应的 CDP 会话目标"
    );
}

/// Ego 无缝切源：主源开始重试时必须保留最后一张有效窗口帧，只有卡片不再可预览时才能释放。
///
/// 数据构造（含关键数值的推导过程）：
///   已缓存窗口帧 = node 7("last-valid") + node 8("removed") = 2 帧
///   本轮仍可预览节点 = {7} = 1 个
///   本轮窗口捕获目标 = 0（模拟 CDP 主源正在尝试）
///   应保留帧数 = 2 - 已删除 node 8(1) = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. CDP 开始重试，但 node 7 仍是画布中的有效 Ego 目标。
///   2. 按全部可预览节点裁剪窗口帧，而不是按当前窗口捕获目标裁剪。
///   3. node 7 真正离开可预览集合后再次裁剪，释放最后有效帧。
///
/// 预期结果：
///   - 正断言：主源重试期间 node 7 的 "last-valid" 仍存在，画面不会退回另一尺寸的旧截图。
///   - 正断言：无关 node 8 被立即释放，避免缓存泄漏。
///   - 负断言：node 7 不得因窗口捕获目标暂时为空而提前消失。
#[test]
fn test_live_frame_retention_uses_all_preview_nodes_during_source_handoff() {
    let mut frames = HashMap::from([(7_u64, "last-valid"), (8_u64, "removed")]);
    let still_previewable = HashSet::from([7_u64]);

    live::retain_frames_for_preview_nodes(&mut frames, &still_previewable);

    assert_eq!(
        frames.get(&7),
        Some(&"last-valid"),
        "CDP 主源重试期间必须保留 node 7 的最后有效窗口帧"
    );
    assert!(
        !frames.contains_key(&8),
        "node 8 已不在可预览集合，旧窗口帧必须被释放"
    );
    assert!(
        frames.contains_key(&7),
        "窗口捕获目标暂时为空不代表 node 7 消失，不能提前清掉其显示帧"
    );

    live::retain_frames_for_preview_nodes(&mut frames, &HashSet::new());

    assert!(
        frames.is_empty(),
        "node 7 真正离开全部预览目标后必须释放最后有效帧，不能永久缓存"
    );
    assert!(
        !frames.contains_key(&7),
        "全部预览目标为空时不得继续保留 node 7 的过期窗口帧"
    );
}
