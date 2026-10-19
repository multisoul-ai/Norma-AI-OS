use super::{AssistantConversation, Message, Speaker};

/// 场景：同一个 Norma 会话连续完成两轮文字或语音对话。
/// 数据构造：依次输入两个不同的问题和两条不同的回复。
/// 执行过程：按发生顺序记录每条消息，读取左栏要展示的完整列表。
/// 预期结果：两轮消息都保留，且角色与顺序均和真实会话一致。
#[test]
fn successive_turns_remain_visible_in_conversation_order() {
    let mut conversation = AssistantConversation::default();
    conversation.user("第一个问题");
    conversation.norma("第一条回复");
    conversation.user("第二个问题");
    conversation.norma("第二条回复");

    assert_eq!(
        conversation.messages(),
        [
            Message {
                speaker: Speaker::User,
                text: "第一个问题".into()
            },
            Message {
                speaker: Speaker::Norma,
                text: "第一条回复".into()
            },
            Message {
                speaker: Speaker::User,
                text: "第二个问题".into()
            },
            Message {
                speaker: Speaker::Norma,
                text: "第二条回复".into()
            },
        ],
        "左栏应保留完整的两轮会话，而不是仅显示最近一轮"
    );
}
