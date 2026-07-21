use super::DialogueControlState;

/// STR-SCHED-1：backlog 已 drain 后若 worker command channel 意外关闭，Finish 不得静默丢失。
///
/// 数据构造（含关键数值的推导过程）：
///   pending characters              = 0
///   submitted-unconsumed characters = 0
///   active characters               = 0 + 0 = 0
///   command capacity                = 8（保持生产 worker handoff 容量）
///   command receivers               = 0（显式 drop 唯一 receiver）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建空 SpeechBacklog 并关闭 input，使其达到 ready-to-finish。
///   2. 安装容量 8 但 receiver 已关闭的 command sender。
///   3. 调用 scheduler 的非阻塞 flush，尝试投递唯一 Finish command。
///
/// 预期结果：
///   - 正断言：flush 返回明确的 command-channel-closed 终止错误。
///   - 负断言：不得把 Finish 标成已排队，也不得以 Ok 假装回复仍可完成。
#[test]
fn test_closed_command_channel_rejects_finish_instead_of_stalling_reply() {
    let mut dialogue = DialogueControlState::default();
    let (commands, receiver) = async_channel::bounded(8);
    drop(receiver);
    dialogue.speech_commands = Some(commands);
    dialogue.speech_backlog.close_input();

    let result = dialogue.flush_speech_commands();

    assert_eq!(
        result.as_ref().err().map(String::as_str),
        Some("TTS command channel closed before the reply drained"),
        "关闭的 worker channel 必须让 Finish flush 返回明确终止错误"
    );
    assert!(result.is_err(), "Finish 发送失败不得返回 Ok 并留下活跃回复");
    assert!(
        !dialogue.speech_finish_queued,
        "receiver 已关闭时不得把未送达的 Finish 标记为 queued"
    );
}
