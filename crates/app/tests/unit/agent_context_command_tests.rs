use super::contextualized_agent_commands;
use agent::{
    AgentCommand, AgentRosterEntry, AgentTrackingMode, CurrentCanvasContext, DesktopContext,
};
use norma::protocol::AgentProfile;

/// ART-CONTEXT-01：生产提交批次必须按名册、画布上下文、用户命令顺序组装。
///
/// 数据构造（含关键数值的推导过程）：
///   roster updates = 1：Bella/Claude
///   canvas updates = 1：workspace=/workspace/norma，cards=0
///   user commands  = 1：“Bella 搜索当前项目”，source=Voice
///   desktop updates = 1：当前工作空间总览 dw-42，无聚焦项目
///   total commands = 1 + 1 + 1 + 1 = 4
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用生产 contextualized_agent_commands seam
///   2. 读取组装后的四个 AgentCommand
///   3. 核对最后一个命令前的上下文更新顺序
///
/// 预期结果：
///   - 正断言：名册、画布、桌面位置、Voice UserTurn 依次排列
///   - 负断言：不得缺失画布上下文或把 UserTurn 放在上下文更新之前
#[test]
fn production_agent_command_batch_updates_context_before_user_turn() {
    let roster = vec![AgentRosterEntry {
        definition_id: 1,
        name: "Bella".to_string(),
        speech_alias: Some("贝拉".to_string()),
        profile: AgentProfile::Claude,
        enabled: true,
        active_node_id: Some(101),
        tracking: AgentTrackingMode::Managed,
        busy: false,
        task_state: agent::AgentTaskState::Ready,
    }];
    let context = CurrentCanvasContext {
        project_name: "norma".to_string(),
        workspace_root: "/workspace/norma".to_string(),
        total_cards: 0,
        cards: Vec::new(),
        conversation_tails: Vec::new(),
    };
    let desktop = DesktopContext::WorkspaceOverview {
        workspace_id: "dw-42".into(),
        workspace_name: "产品研发".into(),
    };

    let commands = contextualized_agent_commands(
        roster.clone(),
        Some(context.clone()),
        Some(desktop.clone()),
        AgentCommand::UserTurn {
            input: "Bella 搜索当前项目".to_string(),
            input_source: agent::AgentInputSource::Voice,
        },
    );

    assert_eq!(commands.len(), 4, "完整生产提交批次应恰好包含 4 条命令");
    assert_eq!(
        commands[0],
        AgentCommand::UpdateAgentRoster(roster),
        "第 1 条命令必须更新本轮 Agent 名册"
    );
    assert_eq!(
        commands[1],
        AgentCommand::UpdateCurrentCanvasContext(context),
        "第 2 条命令必须更新本轮当前画布上下文"
    );
    assert_eq!(
        commands[2],
        AgentCommand::UpdateDesktopContext(Some(desktop)),
        "第 3 条命令必须更新本轮桌面位置，不能沿用旧项目"
    );
    assert_eq!(
        commands[3],
        AgentCommand::UserTurn {
            input: "Bella 搜索当前项目".to_string(),
            input_source: agent::AgentInputSource::Voice,
        },
        "Voice UserTurn 必须连同来源排在全部上下文更新之后"
    );
    assert!(
        !matches!(commands[0], AgentCommand::UserTurn { .. })
            && !matches!(commands[1], AgentCommand::UserTurn { .. })
            && !matches!(commands[2], AgentCommand::UserTurn { .. }),
        "上下文更新完成前不得提前发送 UserTurn"
    );
}
