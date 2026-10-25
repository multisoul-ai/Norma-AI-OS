use super::*;
use runtime::CardAttentionKind;

/// 经典风格的光蛇与呼吸意图与改动前完全一致；其他风格两者都关闭。
///
/// 数据：思考、等待审批、完成未查看三种状态，Agent 卡片。
/// 预期：Classic 下 (chase, breathing) 等于 agent_chase_intent / agent_breathing_intent 的结果
///       （思考 → 光蛇；等待 → Warm；完成未查看 → Cool）；Aura 下一律 (false, Inactive)。
#[test]
fn classic_keeps_existing_intents_and_other_styles_disable_them() {
    let unviewed = Some(CardAttentionState {
        kind: CardAttentionKind::AgentTaskDone,
        viewed: false,
    });
    let cases = [
        (
            Some(AgentTaskState::Thinking),
            None,
            (true, BreathingIntent::Inactive),
        ),
        (
            Some(AgentTaskState::WaitingApproval { tool_name: None }),
            None,
            (false, BreathingIntent::Warm),
        ),
        (
            Some(AgentTaskState::Done),
            unviewed,
            (false, BreathingIntent::Cool),
        ),
    ];
    for (state, attention, expected) in cases {
        assert_eq!(
            classic_intents(RunStyle::Classic, true, state.clone(), attention),
            expected,
            "经典风格 {state:?} 应保持现状"
        );
        assert_eq!(
            classic_intents(RunStyle::Aura, true, state.clone(), attention),
            (false, BreathingIntent::Inactive),
            "灵光风格下 {state:?} 不应再画光蛇或呼吸边框"
        );
    }
    assert_eq!(
        classic_intents(
            RunStyle::Classic,
            false,
            Some(AgentTaskState::Thinking),
            None
        )
        .0,
        false,
        "非 Agent 节点不画光蛇"
    );
}
