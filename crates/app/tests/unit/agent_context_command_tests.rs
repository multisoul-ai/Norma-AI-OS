use super::contextualized_agent_commands;
use agent::{AgentCommand, AgentRosterEntry, AgentTrackingMode, CurrentCanvasContext};
use norma::protocol::AgentProfile;

/// ART-CONTEXT-01：生产提交批次必须按名册、画布上下文、用户命令顺序组装。
///
/// 数据构造（含关键数值的推导过程）：
///   roster updates = 1：Bella/Claude
///   canvas updates = 1：workspace=/workspace/norma，cards=0
///   user commands  = 1：“Bella 搜索当前项目”
///   total commands = 1 + 1 + 1 = 3
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用生产 contextualized_agent_commands seam
///   2. 读取组装后的三个 AgentCommand
///   3. 核对最后一个命令前的上下文更新顺序
///
/// 预期结果：
///   - 正断言：第 1/2/3 项依次为 roster、canvas context、UserTurn
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
    }];
    let context = CurrentCanvasContext {
        project_name: "norma".to_string(),
        workspace_root: "/workspace/norma".to_string(),
        total_cards: 0,
        cards: Vec::new(),
    conversation_tails: Vec::new(),
    };

    let commands = contextualized_agent_commands(
        roster.clone(),
        Some(context.clone()),
        AgentCommand::UserTurn("Bella 搜索当前项目".to_string()),
    );

    assert_eq!(commands.len(), 3, "完整生产提交批次应恰好包含 3 条命令");
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
        AgentCommand::UserTurn("Bella 搜索当前项目".to_string()),
        "UserTurn 必须排在两项上下文更新之后"
    );
    assert!(
        !matches!(commands[0], AgentCommand::UserTurn(_))
            && !matches!(commands[1], AgentCommand::UserTurn(_)),
        "上下文更新完成前不得提前发送 UserTurn"
    );
}
