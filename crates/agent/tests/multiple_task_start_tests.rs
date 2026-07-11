mod support;

use agent::{AgentCommand, AgentEvent, ProviderResponse};
use support::{system, tool, RecordingHost, ScriptedProvider};

/// MTS-1：模型在同一轮返回多个 card_task start 时，系统必须按返回顺序全部入队。
///
/// 数据构造：同一 Dialogue response 含两次 start 与一段虚假的完成文本。
/// 执行过程：提交单个 UserTurn，观察文本事件和 queued 任务数量。
/// 预期结果：两个任务均有 TaskQueued 回执；不产生 DialogueTextDelta，避免未执行的成功声明。
#[test]
fn dialogue_multiple_task_starts_queue_all_without_premature_completion_text() {
    let dialogue = ScriptedProvider::new([ProviderResponse {
        text: "已经完成创建".to_string(),
        tool_calls: vec![
            tool(
                "dialogue-call-1",
                "card_task",
                r#"{"action":"start","intent":"创建终端"}"#,
            ),
            tool(
                "dialogue-call-2",
                "card_task",
                r#"{"action":"start","intent":"再创建一个终端"}"#,
            ),
        ],
    }]);
    let mut system = system(
        dialogue,
        ScriptedProvider::default(),
        RecordingHost::default(),
    );

    let events = system
        .submit(AgentCommand::UserTurn("创建终端".to_string()))
        .expect("带 card_task 的 Dialogue turn 应成功处理");

    assert_eq!(
        system.pending_task_count(),
        2,
        "同一用户 turn 的两个合法 start 都必须进入项目级串行队列"
    );
    assert_eq!(
        events,
        vec![
            AgentEvent::TaskQueued {
                task_id: 1,
                summary: "创建终端".to_string(),
            },
            AgentEvent::TaskQueued {
                task_id: 2,
                summary: "再创建一个终端".to_string(),
            },
        ],
        "每个合法 start 必须产生独立、按模型调用顺序排列的 queued receipt"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AgentEvent::DialogueTextDelta { .. })),
        "任务尚未进入终态时不得播报模型虚假的完成文本"
    );
}

/// MTS-2：同一个稳定 Dialogue tool call id 重复出现时必须复用原 queued receipt。
///
/// 数据构造：同一 Dialogue response 中两次 `card_task start` 使用相同 call id 和 intent，
/// 模拟 Provider 重试或重复 delta 归一化后的重放。
/// 执行过程：提交一次 UserTurn，读取任务数量和返回事件。
/// 预期结果：只入队一次；两个位置都收到同一个 task id 的 queued receipt，绝不创建重复任务。
#[test]
fn duplicate_dialogue_tool_call_id_queues_only_once() {
    let dialogue = ScriptedProvider::new([ProviderResponse {
        text: String::new(),
        tool_calls: vec![
            tool(
                "dialogue-call-duplicate",
                "card_task",
                r#"{"action":"start","intent":"创建终端"}"#,
            ),
            tool(
                "dialogue-call-duplicate",
                "card_task",
                r#"{"action":"start","intent":"创建终端"}"#,
            ),
        ],
    }]);
    let mut system = system(
        dialogue,
        ScriptedProvider::default(),
        RecordingHost::default(),
    );

    let events = system
        .submit(AgentCommand::UserTurn("创建终端".to_string()))
        .expect("重复 tool call id 的 Dialogue turn 仍应可处理");

    assert_eq!(
        system.pending_task_count(),
        1,
        "同一个 stable tool call id 不得生成两项相同任务"
    );
    assert_eq!(
        events,
        vec![AgentEvent::TaskQueued {
            task_id: 1,
            summary: "创建终端".to_string(),
        }],
        "重复调用只能产生首次真实入队的 TaskQueued 事件"
    );
}

/// MTS-3：不同用户回合重用同名 Provider call id 时，两个合法 start 仍须分别入队。
///
/// 数据构造：Dialogue Provider 的两份独立 response 都使用 `dialogue-call-1`，但 intent 分别为
/// 创建第一个和第二个终端，模拟 Provider 只保证单个 response 内 id 唯一的行为。
/// 执行过程：连续提交两个 UserTurn，读取每轮事件及累计 pending 数。
/// 预期结果：两轮都产生新的 TaskQueued，累计两项任务；当前回合去重不得污染后续回合。
#[test]
fn reused_provider_call_id_in_later_turn_queues_a_new_task() {
    let dialogue = ScriptedProvider::new([
        ProviderResponse {
            text: String::new(),
            tool_calls: vec![tool(
                "dialogue-call-1",
                "card_task",
                r#"{"action":"start","intent":"创建第一个终端"}"#,
            )],
        },
        ProviderResponse {
            text: String::new(),
            tool_calls: vec![tool(
                "dialogue-call-1",
                "card_task",
                r#"{"action":"start","intent":"创建第二个终端"}"#,
            )],
        },
    ]);
    let mut system = system(
        dialogue,
        ScriptedProvider::default(),
        RecordingHost::default(),
    );

    let first = system
        .submit(AgentCommand::UserTurn("创建第一个终端".to_string()))
        .expect("首轮应入队第一个任务");
    let second = system
        .submit(AgentCommand::UserTurn("创建第二个终端".to_string()))
        .expect("下一轮重用 call id 仍应入队新任务");

    assert_eq!(
        first,
        vec![AgentEvent::TaskQueued {
            task_id: 1,
            summary: "创建第一个终端".to_string(),
        }],
        "首轮应保留自己的 queued receipt"
    );
    assert_eq!(
        second,
        vec![AgentEvent::TaskQueued {
            task_id: 2,
            summary: "创建第二个终端".to_string(),
        }],
        "下一轮不得复用前一轮 task id 或 receipt"
    );
    assert_eq!(
        system.pending_task_count(),
        2,
        "两个独立用户回合的合法 start 必须积累为两项待执行任务"
    );
}
