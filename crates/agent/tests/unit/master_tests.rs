use super::*;
use crate::{AdapterCommand, AdapterError, AdapterResult};
use futures::future::BoxFuture;

struct NoopAdapter;

impl AgentCardAdapter for NoopAdapter {
    fn execute<'a>(
        &'a self,
        _command: AdapterCommand,
        _context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async { Ok(AdapterResult::succeeded("unused")) })
    }
}

/// HISTORY-1：连续回合必须用 Rig 返回的完整历史替换旧快照，不能重复追加
///
/// 数据构造（含关键数值的推导过程）：
///   first snapshot  = 2 messages（user-1 + assistant-1）
///   second snapshot = 4 messages（前 2 条 + user-2 + assistant-2）
///   third snapshot  = 6 messages（前 4 条 + user-3 + assistant-3）
///   expected final  = 6，而不是 2 + 4 + 6 = 12 messages
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 注入第一轮完整快照
///   2. 用第二轮完整快照替换第一轮
///   3. 用第三轮完整快照再次替换
///
/// 预期结果：
///   - 正断言：最终只保留六条按顺序排列的消息
///   - 负断言：第一轮 user/assistant 各只出现一次，不发生历史重复膨胀
#[test]
fn complete_rig_history_replaces_previous_snapshot() {
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        "https://provider.example/v1",
        "model-a",
        Some("test-key".to_string()),
    );
    let mut session = NormaSession::new("project-a", &config, Arc::new(NoopAdapter))
        .expect("a syntactically valid provider config should create a session without IO");
    let first = vec![Message::user("user-1"), Message::assistant("assistant-1")];
    let second = vec![
        Message::user("user-1"),
        Message::assistant("assistant-1"),
        Message::user("user-2"),
        Message::assistant("assistant-2"),
    ];
    let third = vec![
        Message::user("user-1"),
        Message::assistant("assistant-1"),
        Message::user("user-2"),
        Message::assistant("assistant-2"),
        Message::user("user-3"),
        Message::assistant("assistant-3"),
    ];

    session.replace_protocol_history(first);
    session.replace_protocol_history(second);
    session.replace_protocol_history(third);

    assert_eq!(
        session.history_len(),
        6,
        "three turns must contain exactly six messages"
    );
    assert_eq!(
        session
            .protocol_history
            .iter()
            .filter(|message| **message == Message::user("user-1"))
            .count(),
        1,
        "the first user message must appear exactly once"
    );
    assert_eq!(
        session
            .protocol_history
            .iter()
            .filter(|message| **message == Message::assistant("assistant-1"))
            .count(),
        1,
        "the first assistant message must appear exactly once"
    );
    assert_eq!(
        session.protocol_history[5],
        Message::assistant("assistant-3"),
        "the latest reply must remain final"
    );
    assert_ne!(
        session.history_len(),
        12,
        "complete histories must not be appended as deltas"
    );
}

/// STREAM-1：分片到达的 think 标签与推理正文不能泄漏到可见增量。
///
/// 数据构造（含关键数值的推导过程）：
///   chunks = 3（"答复<thi" + "nk>秘密</thi" + "nk>完成"）
///   raw length > visible length，最终可见文本 = "答复完成"
///   hidden blocks = 1，泄漏字符数应为 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 首片只出现 `<think>` 的前缀，先扣住不发送
///   2. 第二片进入推理块并继续丢弃未闭合正文
///   3. 第三片闭合标签后恢复输出正文
///
/// 预期结果：
///   - 正断言：各阶段可见快照保持单调，最终为“答复完成”
///   - 负断言：标签片段和“秘密”均不出现在任何可见快照
#[test]
fn streaming_visibility_holds_split_thinking_blocks() {
    let first = visible_stream_snapshot("答复<thi");
    let second = visible_stream_snapshot("答复<think>秘密</thi");
    let final_snapshot = visible_stream_snapshot("答复<think>秘密</think>完成");

    assert_eq!(
        first, "答复",
        "a partial opening think tag must be held back"
    );
    assert_eq!(
        second, "答复",
        "unclosed reasoning must add no visible text"
    );
    assert_eq!(
        final_snapshot, "答复完成",
        "visible output must resume after reasoning"
    );
    assert!(
        !final_snapshot.contains("秘密"),
        "reasoning content must never be visible"
    );
    assert!(
        !final_snapshot.contains("think"),
        "reasoning tags must never be visible"
    );
}

/// STREAM-2：工具轮的前置承诺文本必须丢弃，只发布最终无工具轮的可信文本。
///
/// 数据构造（含关键数值的推导过程）：
///   tool preface chunks = 1（“我来处理”）
///   final response chunks = 2（“已” + “创建”）
///   expected published = 2 chunks，preface published = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 暂存工具轮前置文本
///   2. 模拟 ToolExecutionStart 清空本轮暂存
///   3. 暂存最终轮文本并按最终答复完成增量
///
/// 预期结果：
///   - 正断言：发布增量只重建“已创建”
///   - 负断言：“我来处理”不进入发布列表或最终拼接文本
#[test]
fn tool_turn_preface_is_discarded_before_final_stream_is_published() {
    let mut pending = vec!["我来处理".to_string()];
    pending.clear();
    pending.push("已".to_string());
    pending.push("创建".to_string());

    let published = finalize_stream_deltas("已创建", "已创建", pending);

    assert_eq!(
        published.len(),
        2,
        "the final response should preserve its two safe chunks"
    );
    assert_eq!(
        published.concat(),
        "已创建",
        "safe chunks must reconstruct the final response"
    );
    assert!(
        !published.iter().any(|chunk| chunk.contains("我来处理")),
        "tool preface must never be published"
    );
    assert_ne!(
        published.concat(),
        "我来处理已创建",
        "tool preface must not prefix the final response"
    );
}

/// HISTORY-2：长会话裁剪必须从真实 user 回合边界开始，不能留下孤立 tool result。
///
/// 数据构造（含关键数值的推导过程）：
///   completed turns = 60
///   messages per turn = 2（user + assistant）
///   total = 60 × 2 = 120 messages
///   budget = 96 messages，因此至少移除 120 - 96 = 24 条
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造六十轮完整协议历史
///   2. 以 Rig 完整快照替换当前历史并触发裁剪
///   3. 检查长度、起始角色和最新回复
///
/// 预期结果：
///   - 正断言：裁剪后不超过 96 条，且从 user 消息开始
///   - 正断言：最后一轮 assistant 回复仍然保留
///   - 负断言：最旧 user-0 已移除，不会无限增长
#[test]
fn protocol_history_prunes_at_human_turn_boundaries() {
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        "https://provider.example/v1",
        "model-a",
        Some("test-key".to_string()),
    );
    let mut session = NormaSession::new("project-a", &config, Arc::new(NoopAdapter))
        .expect("a syntactically valid provider config should create a session without IO");
    let mut complete_history = Vec::new();
    for turn in 0..60 {
        complete_history.push(Message::user(format!("user-{turn}")));
        complete_history.push(Message::assistant(format!("assistant-{turn}")));
    }

    session.replace_protocol_history(complete_history);

    assert!(
        session.history_len() <= MAX_PROTOCOL_MESSAGES,
        "history must respect its budget"
    );
    assert!(
        is_human_user_message(&session.protocol_history[0]),
        "history must begin at a human turn"
    );
    assert_eq!(
        session.protocol_history.last(),
        Some(&Message::assistant("assistant-59")),
        "newest reply must remain"
    );
    assert!(
        !session.protocol_history.contains(&Message::user("user-0")),
        "oldest turn must be removed"
    );
}

/// MODEL-1：同一项目切换 Rig 模型时保留既有协议历史
///
/// 数据构造（含关键数值的推导过程）：
///   initial history = 2 messages（1 条 user + 1 条 assistant）
///   model switches  = 1（model-a → model-b）
///   expected history = 2 + 0 = 2 messages
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建无需联网的 OpenAI-compatible NormaSession
///   2. 注入一轮已经完成的协议历史
///   3. 调用 `switch_model` 构造并替换 Rig 模型
///
/// 预期结果：
///   - 正断言：切换成功且两条历史完整保留
///   - 负断言：切换不得清空会话或创建空历史
#[test]
fn switch_model_preserves_protocol_history() {
    let initial = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        "https://provider-a.example/v1",
        "model-a",
        Some("test-key-a".to_string()),
    );
    let replacement = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        "https://provider-b.example/v1",
        "model-b",
        Some("test-key-b".to_string()),
    );
    let mut session = NormaSession::new("project-a", &initial, Arc::new(NoopAdapter))
        .expect("a syntactically valid provider config should create a session without IO");
    session.protocol_history = vec![
        Message::user("你好"),
        Message::assistant("你好，我是 Norma"),
    ];

    session
        .switch_model(&replacement)
        .expect("a valid replacement model should be accepted");

    assert_eq!(
        session.history_len(),
        2,
        "switching models must preserve two messages"
    );
    assert_eq!(
        session.protocol_history[0],
        Message::user("你好"),
        "user history must remain"
    );
    assert_eq!(
        session.protocol_history[1],
        Message::assistant("你好，我是 Norma"),
        "assistant history must remain"
    );
}

/// HISTORY-3：消息数量很少但正文超过模型上下文预算时，也必须按完整回合裁剪。
///
/// 数据构造（含关键数值的推导过程）：
///   context window = 256 tokens
///   history budget = 256 × 3 / 4 × 2 chars/token = 384 bytes
///   turns = 2；第一轮正文 = 160 chars，第二轮正文 = 12 chars
///   messages = 4 < 96-message hard cap
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建仅有 64-token 上下文窗口的会话
///   2. 注入一轮超长历史和一轮短历史
///   3. 触发替换与字符预算裁剪
///
/// 预期结果：
///   - 正断言：短的最新一轮 user/assistant 配对完整保留
///   - 负断言：超长旧回合被整体删除，历史不从 assistant 或 tool result 开始
#[test]
fn protocol_history_respects_context_char_budget_below_message_cap() {
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        "https://provider.example/v1",
        "small-model",
        Some("test-key".to_string()),
    )
    .with_context_window_tokens(256);
    let mut session = NormaSession::new("project-a", &config, Arc::new(NoopAdapter))
        .expect("小上下文的合法配置应能创建会话且无需网络 IO");
    let oversized = "超".repeat(160);
    let history = vec![
        Message::user(oversized.clone()),
        Message::assistant("旧回复"),
        Message::user("最新问题"),
        Message::assistant("最新回复"),
    ];

    session.replace_protocol_history(history);

    assert_eq!(
        session.history_len(),
        2,
        "字符预算应整体裁掉第一轮并保留两条最新消息"
    );
    assert_eq!(
        session.protocol_history[0],
        Message::user("最新问题"),
        "裁剪后的历史必须从最新 human user 回合开始"
    );
    assert_eq!(
        session.protocol_history[1],
        Message::assistant("最新回复"),
        "最新 assistant 回复必须与 user 成对保留"
    );
    assert!(
        !session.protocol_history.contains(&Message::user(oversized)),
        "超过 96-char 历史预算的旧 user 回合必须删除"
    );
    assert!(
        !matches!(
            session.protocol_history.first(),
            Some(Message::Assistant { .. })
        ),
        "历史不得从孤立 assistant 消息开始"
    );
}
