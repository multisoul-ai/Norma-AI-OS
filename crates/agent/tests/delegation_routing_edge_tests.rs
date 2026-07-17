pub mod support;

use agent::{AgentCommand, AgentEvent, AgentRosterEntry, AgentTrackingMode};
use norma::protocol::AgentProfile;
use support::{system, text_response, RecordingHost, ScriptedProvider};

fn bella_roster() -> Vec<AgentRosterEntry> {
    vec![AgentRosterEntry {
        definition_id: 1,
        name: "Bella".to_string(),
        speech_alias: Some("贝拉".to_string()),
        profile: AgentProfile::Claude,
        enabled: true,
        active_node_id: Some(101),
        tracking: AgentTrackingMode::Managed,
        busy: false,
    }]
}

/// DP-9：动作词用于评价或状态询问时，不得被句首名称路由成执行任务。
///
/// 数据构造（含关键数值的推导过程）：
///   ordinary statements = 3：“测试能力很强” + “分析得很好” + “测试结束了吗”
///   addressed tasks = 0
///   provider tool calls = 0，每轮只返回可区分的普通文本
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 三个独立 AgentSystem 注入相同 Bella 名册
///   2. 分别提交能力评价、完成质量评价和状态询问
///   3. 观察 DialogueTextDelta 与 TaskQueued
///
/// 预期结果：
///   - 正断言：三轮都保留对应 Dialogue 文本
///   - 负断言：任何一轮都不得产生 TaskQueued
#[test]
fn action_words_in_evaluations_and_status_questions_stay_in_dialogue() {
    let mut capability_system = system(
        ScriptedProvider::new([text_response("能力评价")]),
        ScriptedProvider::default(),
        RecordingHost::default(),
    );
    capability_system
        .set_agent_roster(bella_roster())
        .expect("Bella 名册应可用于能力评价保护");
    let capability_events = capability_system
        .submit(AgentCommand::UserTurn("Bella 测试能力很强".to_string()))
        .expect("能力评价应保留在 Dialogue");

    let mut quality_system = system(
        ScriptedProvider::new([text_response("质量评价")]),
        ScriptedProvider::default(),
        RecordingHost::default(),
    );
    quality_system
        .set_agent_roster(bella_roster())
        .expect("Bella 名册应可用于质量评价保护");
    let quality_events = quality_system
        .submit(AgentCommand::UserTurn("Bella 分析得很好".to_string()))
        .expect("质量评价应保留在 Dialogue");

    let mut status_system = system(
        ScriptedProvider::new([text_response("状态查询")]),
        ScriptedProvider::default(),
        RecordingHost::default(),
    );
    status_system
        .set_agent_roster(bella_roster())
        .expect("Bella 名册应可用于状态询问保护");
    let status_events = status_system
        .submit(AgentCommand::UserTurn("Bella，测试结束了吗".to_string()))
        .expect("测试状态询问应保留在 Dialogue");

    assert!(
        capability_events.iter().any(|event| matches!(
            event,
            AgentEvent::DialogueTextDelta { text } if text == "能力评价"
        )),
        "测试能力评价必须保留普通 Dialogue 文本"
    );
    assert!(
        quality_events.iter().any(|event| matches!(
            event,
            AgentEvent::DialogueTextDelta { text } if text == "质量评价"
        )),
        "分析质量评价必须保留普通 Dialogue 文本"
    );
    assert!(
        status_events.iter().any(|event| matches!(
            event,
            AgentEvent::DialogueTextDelta { text } if text == "状态查询"
        )),
        "测试结束状态询问必须保留普通 Dialogue 文本"
    );
    assert!(
        !capability_events
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskQueued { .. })),
        "测试能力评价不得产生 TaskQueued"
    );
    assert!(
        !quality_events
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskQueued { .. })),
        "分析质量评价不得产生 TaskQueued"
    );
    assert!(
        !status_events
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskQueued { .. })),
        "测试状态询问不得产生 TaskQueued"
    );
}

/// DP-10：明确寻址的执行正文可以包含“为什么”或“怎么”，不得被说明词表吞掉。
///
/// 数据构造（含关键数值的推导过程）：
///   direct targets = 1：Bella
///   actionable requests = 2：分析失败原因 + 调查复现方法
///   dialogue tool calls = 0，两轮都模拟模型漏调 card_task
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别提交“Bella，分析为什么测试失败”和“Bella，调查怎么复现这个 bug”
///   2. 路由先识别句首 Bella + 明确动作，再处理正文说明词
///   3. 观察确定性 fallback
///
/// 预期结果：
///   - 正断言：两轮都恰好产生一个 TaskQueued
///   - 负断言：不得展示模型漏调后的普通建议文本
#[test]
fn direct_action_can_include_explanation_words_in_its_task_body() {
    let mut why_system = system(
        ScriptedProvider::new([text_response("我可以解释测试失败的一般原因")]),
        ScriptedProvider::default(),
        RecordingHost::default(),
    );
    why_system
        .set_agent_roster(bella_roster())
        .expect("Bella 名册应可用于失败原因任务");
    let why_events = why_system
        .submit(AgentCommand::UserTurn(
            "Bella，分析为什么测试失败".to_string(),
        ))
        .expect("明确分析失败原因应进入 CardTask");

    let mut how_system = system(
        ScriptedProvider::new([text_response("我可以提供通用复现建议")]),
        ScriptedProvider::default(),
        RecordingHost::default(),
    );
    how_system
        .set_agent_roster(bella_roster())
        .expect("Bella 名册应可用于复现调查任务");
    let how_events = how_system
        .submit(AgentCommand::UserTurn(
            "Bella，调查怎么复现这个 bug".to_string(),
        ))
        .expect("明确调查复现方法应进入 CardTask");

    assert_eq!(
        why_events
            .iter()
            .filter(|event| matches!(event, AgentEvent::TaskQueued { .. }))
            .count(),
        1,
        "明确分析为什么失败必须产生唯一 TaskQueued"
    );
    assert_eq!(
        how_events
            .iter()
            .filter(|event| matches!(event, AgentEvent::TaskQueued { .. }))
            .count(),
        1,
        "明确调查怎么复现必须产生唯一 TaskQueued"
    );
    assert!(
        !why_events
            .iter()
            .any(|event| matches!(event, AgentEvent::DialogueTextDelta { .. })),
        "失败原因任务 fallback 后不得展示普通解释文本"
    );
    assert!(
        !how_events
            .iter()
            .any(|event| matches!(event, AgentEvent::DialogueTextDelta { .. })),
        "复现调查任务 fallback 后不得展示普通建议文本"
    );
}
