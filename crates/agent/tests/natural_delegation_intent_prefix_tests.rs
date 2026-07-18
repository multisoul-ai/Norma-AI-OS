pub mod support;

use agent::{AgentCommand, AgentEvent, AgentRosterEntry, AgentTrackingMode};
use norma::protocol::{AgentProfile, CardCommand};
use support::{system, text_response, RecordingHost, ScriptedProvider};

fn bella_roster() -> Vec<AgentRosterEntry> {
    vec![AgentRosterEntry {
        definition_id: 98,
        name: "Bella".to_string(),
        speech_alias: Some("贝拉".to_string()),
        profile: AgentProfile::Codex,
        enabled: true,
        active_node_id: Some(64),
        tracking: AgentTrackingMode::Managed,
        busy: false,
    }]
}

/// DP-8：带“我想让”意图前缀的自然委派在模型只输出思考文本时仍必须确定性入队。
///
/// 数据构造（含关键数值的推导过程）：
///   roster agents        = 1：Bella，语音别名“贝拉”
///   dialogue tool calls  = 0：重放现场中只有 `<think>` 文本的 Provider 响应
///   direct turns         = 1：“让贝拉查看当前项目的目录结构”
///   prefixed turns       = 1：“我想让贝拉查看当前项目的目录结构”
///   expected queued each = 1：两种说法语义相同，均应命中本地 fallback
///   pump upper bound     = 4：启动(1) + 派发(1) + 回执聚合(1) + 保护 tick(1)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 为两个独立 AgentSystem 注入相同 Bella 名册
///   2. Provider 对两轮都只返回现场同形的思考文本，不调用 card_task
///   3. 分别提交直接委派和带意图前缀的自然委派
///   4. 推进带前缀任务并检查到达 Host 的结构化 assignment
///
/// 预期结果：
///   - 正断言：直接委派与带“我想让”前缀的委派都各产生一个 TaskQueued
///   - 正断言：Bella 收到的任务正文只保留“查看当前项目的目录结构”
///   - 负断言：两轮都不得把内部思考文本当作普通可见回复
#[test]
fn intent_prefixed_agent_delegation_falls_back_when_provider_omits_tool_call() {
    let think_only = "<think>用户想让贝拉查看当前项目的目录结构，需要使用 card_task。</think>";
    let mut direct_system = system(
        ScriptedProvider::new([text_response(think_only)]),
        ScriptedProvider::default(),
        RecordingHost::default(),
    );
    direct_system
        .set_agent_roster(bella_roster())
        .expect("直接委派测试必须可注入 Bella/贝拉 名册");
    let direct_events = direct_system
        .submit(AgentCommand::UserTurn(
            "让贝拉查看当前项目的目录结构".to_string(),
        ))
        .expect("直接委派必须由本地 fallback 安全接管");

    let mut prefixed_system = system(
        ScriptedProvider::new([text_response(think_only)]),
        ScriptedProvider::default(),
        RecordingHost::default(),
    );
    prefixed_system
        .set_agent_roster(bella_roster())
        .expect("意图前缀测试必须可注入相同 Bella/贝拉 名册");
    let prefixed_events = prefixed_system
        .submit(AgentCommand::UserTurn(
            "我想让贝拉查看当前项目的目录结构".to_string(),
        ))
        .expect("自然意图前缀不应阻止明确 Bella 委派");
    // 快路径最多需要：启动(1) + 派发(1) + 回执聚合(1) + 保护 tick(1) = 4。
    for _ in 0..4 {
        let _ = prefixed_system.pump();
    }

    assert_eq!(
        direct_events
            .iter()
            .filter(|event| matches!(event, AgentEvent::TaskQueued { .. }))
            .count(),
        1,
        "直接‘让贝拉’委派必须产生且只产生一个 TaskQueued"
    );
    assert_eq!(
        prefixed_events
            .iter()
            .filter(|event| matches!(event, AgentEvent::TaskQueued { .. }))
            .count(),
        1,
        "带‘我想让’前缀的同义委派也必须产生且只产生一个 TaskQueued"
    );
    assert_eq!(
        prefixed_system.card_tool_host().commands.len(),
        1,
        "带意图前缀的委派必须只执行一次确定性派发"
    );
    let CardCommand::DispatchAgentTasks { assignments, .. } =
        &prefixed_system.card_tool_host().commands[0].command
    else {
        panic!("带意图前缀的委派必须直接执行 DispatchAgentTasks");
    };
    assert_eq!(assignments.len(), 1, "确定性派发必须只包含 Bella 一项目标");
    assert_eq!(assignments[0].name, "Bella", "派发目标必须规范化为 Bella");
    assert_eq!(
        assignments[0].task, "查看当前项目的目录结构",
        "任务正文必须移除‘我想让贝拉’寻址前缀，只保留实际工作"
    );
    assert!(
        !direct_events
            .iter()
            .chain(prefixed_events.iter())
            .any(|event| matches!(event, AgentEvent::DialogueTextDelta { text } if text.contains("<think>"))),
        "内部思考文本不得成为直接或意图前缀委派的用户可见回复"
    );
}
