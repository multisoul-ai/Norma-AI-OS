use super::artifacts_from_agent_events;
use agent::AgentEvent;

/// ART-1: 对话模型把内部推理包在 `<think>` 标签中时，TTS 只能收到标签外的正文。
///
/// 数据构造：
///   流式事件按真实网络分块拆开：第一个分块含 `<think>` 和英文内部推理，第二个分块闭合
///   标签，第三个分块为用户可听到的英文答复。
///
/// 执行过程：
///   1. 将三个 DialogueTextDelta 交给 Agent 运行时的 reply 提取边界；
///   2. 读取产出的 VoiceTurnArtifacts.reply_text。
///
/// 预期结果：
///   - 正断言：标签外的英文正文逐字保留，不能因为是英文而被过滤；
///   - 负断言：内部 `<think>` 推理及其标签不得进入中文 TTS，避免产生 OOV 警告。
#[test]
fn test_agent_reply_strips_thinking_block_without_dropping_english_answer() {
    let artifacts = artifacts_from_agent_events(vec![
        AgentEvent::DialogueTextDelta {
            text: "<think>The user asked for an English answer.".into(),
        },
        AgentEvent::DialogueTextDelta {
            text: " I should answer directly.</think>".into(),
        },
        AgentEvent::DialogueTextDelta {
            text: "Please open README.md and follow the setup steps.".into(),
        },
    ]);

    assert_eq!(
        artifacts.reply_text, "Please open README.md and follow the setup steps.",
        "清洗只能移除 think 块，标签外英文答复必须完整保留"
    );
    assert!(
        !artifacts.reply_text.contains("<think>")
            && !artifacts.reply_text.contains("The user asked"),
        "内部推理不得作为 TTS 输入；实际 reply: {:?}",
        artifacts.reply_text
    );
}
