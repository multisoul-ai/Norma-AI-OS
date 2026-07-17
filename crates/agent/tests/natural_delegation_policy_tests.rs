pub mod support;

use agent::{AgentCommand, AgentEvent, AgentRosterEntry, AgentTrackingMode};
use norma::protocol::AgentProfile;
use support::{system, text_response, RecordingHost, ScriptedProvider};

fn roster(entries: &[(&str, AgentProfile)]) -> Vec<AgentRosterEntry> {
    entries
        .iter()
        .enumerate()
        .map(|(index, (name, profile))| AgentRosterEntry {
            definition_id: (index + 1) as i64,
            name: (*name).to_string(),
            speech_alias: None,
            profile: *profile,
            enabled: true,
            active_node_id: Some((index + 101) as u64),
            tracking: AgentTrackingMode::Managed,
            busy: false,
        })
        .collect()
}

/// DP-5：无标点的句首直接寻址应 fallback，同名普通陈述不得执行。
///
/// 数据构造（含关键数值的推导过程）：
///   roster agents = 2：Bella + Alan
///   direct targets = 1：Bella，后接“你去搜索”动作
///   mention targets = 0：“Bella 是一个好名字”只是陈述
///   provider tool calls = 0，两次都只返回文本
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向第一个 AgentSystem 注入名册并提交无标点直接寻址
///   2. Provider 漏调工具 → 确定性路由应补入 CardTask
///   3. 向第二个 AgentSystem 提交句首同名普通陈述
///
/// 预期结果：
///   - 正断言：直接寻址恰好产生 1 个 TaskQueued
///   - 正断言：普通陈述保留 DialogueTextDelta
///   - 负断言：普通陈述不产生任务，直接寻址不播放漏调建议
#[test]
fn unpunctuated_direct_agent_address_falls_back_without_triggering_name_statements() {
    let agents = roster(&[
        ("Bella", AgentProfile::Claude),
        ("Alan", AgentProfile::Codex),
    ]);
    let mut direct_system = system(
        ScriptedProvider::new([text_response("请先说“指派任务”")]),
        ScriptedProvider::default(),
        RecordingHost::default(),
    );
    direct_system
        .set_agent_roster(agents.clone())
        .expect("Bella/Alan 名册应可用于直接寻址");
    let direct_events = direct_system
        .submit(AgentCommand::UserTurn("Bella 你去搜索当前项目".to_string()))
        .expect("无标点直接寻址应进入 CardTask");

    let mut statement_system = system(
        ScriptedProvider::new([text_response("Bella 确实是一个简短名字")]),
        ScriptedProvider::default(),
        RecordingHost::default(),
    );
    statement_system
        .set_agent_roster(agents)
        .expect("Bella/Alan 名册应可用于误触发保护");
    let statement_events = statement_system
        .submit(AgentCommand::UserTurn("Bella 是一个好名字".to_string()))
        .expect("句首同名普通陈述应留在 Dialogue");

    assert_eq!(
        direct_events
            .iter()
            .filter(|event| matches!(event, AgentEvent::TaskQueued { .. }))
            .count(),
        1,
        "无标点直接寻址必须且只能产生一个 TaskQueued"
    );
    assert!(
        !direct_events.iter().any(|event| matches!(
            event,
            AgentEvent::DialogueTextDelta { text } if text.contains("指派任务")
        )),
        "直接寻址 fallback 后不得继续要求用户说“指派任务”"
    );
    assert!(
        statement_events.iter().any(|event| matches!(
            event,
            AgentEvent::DialogueTextDelta { text } if text.contains("简短名字")
        )),
        "句首同名普通陈述应保留 Dialogue Provider 回答"
    );
    assert!(
        !statement_events
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskQueued { .. })),
        "句首同名普通陈述不得产生 Agent 任务"
    );
}

/// DP-6：句首多 Agent 集合寻址应在 Dialogue 模型漏调时进入 CardTask。
///
/// 数据构造（含关键数值的推导过程）：
///   roster agents = 2：Bella + Alan
///   addressed set = 2：“Bella 和 Alan”
///   shared task   = 1：“搜索当前项目”，由“你们都”标记共同派发
///   provider tool calls = 0，只返回多余确认文本
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 注入 Bella/Alan 名册
///   2. 提交句首集合寻址指令
///   3. Provider 没有调用 card_task，观察公开 AgentEvent
///
/// 预期结果：
///   - 正断言：恰好产生 1 个 TaskQueued，保留整条原始共同任务意图
///   - 负断言：不得播放“需要分别指定”的旧确认文本
#[test]
fn multi_agent_shared_task_address_falls_back_when_dialogue_model_misses_tool() {
    let mut agent_system = system(
        ScriptedProvider::new([text_response("请分别指定每个 Agent 的任务")]),
        ScriptedProvider::default(),
        RecordingHost::default(),
    );
    agent_system
        .set_agent_roster(roster(&[
            ("Bella", AgentProfile::Claude),
            ("Alan", AgentProfile::Codex),
        ]))
        .expect("Bella/Alan 名册应可用于集合寻址");

    let events = agent_system
        .submit(AgentCommand::UserTurn(
            "Bella 和 Alan，你们都搜索当前项目".to_string(),
        ))
        .expect("明确共同任务应进入 CardTask");

    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AgentEvent::TaskQueued { .. }))
            .count(),
        1,
        "句首多 Agent 集合寻址必须且只能产生一个 TaskQueued"
    );
    assert!(
        !events.iter().any(|event| matches!(
            event,
            AgentEvent::DialogueTextDelta { text } if text.contains("分别指定")
        )),
        "共同任务 fallback 后不得继续要求用户补充独立分工"
    );
}

/// DP-7：名称有前缀重叠时，多 Agent 集合寻址必须优先匹配最长名称。
///
/// 数据构造（含关键数值的推导过程）：
///   roster agents = 3：小安 + 小安娜 + Alan
///   addressed set = 2：小安娜 + Alan
///   prefix collision = 1：“小安”是“小安娜”的严格前缀
///   provider tool calls = 0，只返回不应展示的确认文本
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 注入含前缀重叠名称的名册
///   2. 提交“小安娜 和 Alan”共同任务
///   3. Provider 漏调工具，观察确定性 fallback
///
/// 预期结果：
///   - 正断言：完整名称“小安娜”被识别，恰好产生 1 个 TaskQueued
///   - 负断言：不得把“小安娜”截成“小安”后因剩余“娜”而漏派发
#[test]
fn multi_agent_address_prefers_longest_overlapping_name() {
    let mut agent_system = system(
        ScriptedProvider::new([text_response("请换一种说法")]),
        ScriptedProvider::default(),
        RecordingHost::default(),
    );
    agent_system
        .set_agent_roster(roster(&[
            ("小安", AgentProfile::Claude),
            ("小安娜", AgentProfile::Codex),
            ("Alan", AgentProfile::Codex),
        ]))
        .expect("前缀重叠但完整名称唯一的名册应合法");

    let events = agent_system
        .submit(AgentCommand::UserTurn(
            "小安娜 和 Alan，你们一起检查当前项目".to_string(),
        ))
        .expect("最长名称参与的集合寻址应进入 CardTask");

    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AgentEvent::TaskQueued { .. }))
            .count(),
        1,
        "应先匹配完整名称小安娜，并为共同任务产生一个 TaskQueued"
    );
    assert!(
        !events.iter().any(|event| matches!(
            event,
            AgentEvent::DialogueTextDelta { text } if text.contains("换一种说法")
        )),
        "名称前缀冲突不得导致明确共同任务退化为确认文本"
    );
}
