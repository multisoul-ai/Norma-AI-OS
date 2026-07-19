use agent::visible_norma_reply;

/// REPLY-1：Provider 把内部推理放进 think 标签时，Norma 最终答复只保留可见正文
///
/// 数据构造（含关键数值的推导过程）：
///   Complete think blocks = 1（内部中文推理）
///   Visible answers = 1（“任务已派发给 Alan。”）
///   TTS-visible think blocks = 1 - 1 stripped = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 输入真实 MiniMax 风格的 `<think>...</think>` 加正文
///   2. NormaSession 的最终答复边界移除完整推理块
///   3. 对剩余正文做首尾空白归一化
///
/// 预期结果：
///   - 正断言：正文逐字保留
///   - 负断言：think 标签及内部推理不进入最终答复或 TTS
#[test]
fn final_norma_reply_strips_provider_thinking_blocks() {
    let raw = "<think>工具已经返回 accepted，我只能报告接收。</think>\n\n任务已派发给 Alan。";

    let visible = visible_norma_reply(raw);

    assert_eq!(
        visible, "任务已派发给 Alan。",
        "the user-visible final answer should preserve only the provider's answer text"
    );
    assert!(
        !visible.contains("<think>") && !visible.contains("accepted"),
        "provider reasoning must not leak into UI history or TTS"
    );
}

/// REPLY-2：未闭合 think 块不能把内部推理作为失败兜底泄露给用户
///
/// 数据构造（含关键数值的推导过程）：
///   Opening tags = 1
///   Closing tags = 0
///   Safe visible characters after opening tag = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 输入只有起始标签的截断 Provider 文本
///   2. 清洗器保留标签前正文并丢弃未闭合块之后的内容
///
/// 预期结果：
///   - 正断言：返回空字符串，让上层走明确的空答复错误
///   - 负断言：截断推理内容不得泄露
#[test]
fn unclosed_thinking_block_is_discarded_instead_of_leaked() {
    let visible = visible_norma_reply("<think>尚未完成的内部推理");

    assert_eq!(
        visible, "",
        "an unclosed reasoning block should produce no user-visible content"
    );
    assert!(
        !visible.contains("内部推理"),
        "truncated provider reasoning must not leak into the final response"
    );
}

/// REPLY-3：Norma 的用户可见回复不能超过十五个 Unicode 字符。
///
/// 数据构造（含关键数值的推导过程）：
///   原始可见正文 = 17 字（“诺诺知道啦，马上帮你处理这个事情哦”）
///   正文预算 = 15 字
///   截断策略 = 前 14 字 + 1 个省略号 = 15 字
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Provider 返回一段超过十五字的口语回复
///   2. 最终可见答复边界保留前十四字并追加省略号
///   3. UI 与 TTS 消费同一条收口后的文本
///
/// 预期结果：
///   - 正断言：输出恰好为十五字且以省略号自然收口
///   - 负断言：第十五字之后的“事情哦”不得进入 UI 或 TTS
#[test]
fn final_norma_reply_never_exceeds_fifteen_unicode_characters() {
    let visible = visible_norma_reply("诺诺知道啦，马上帮你处理这个事情哦");

    assert_eq!(
        visible, "诺诺知道啦，马上帮你处理这个…",
        "超长回复必须保留前十四字并用一个省略号收口"
    );
    assert_eq!(
        visible.chars().count(),
        15,
        "用户可见回复必须恰好受十五个 Unicode 字符上限约束"
    );
    assert!(visible.ends_with('…'), "被截断的回复必须以省略号结束");
    assert!(
        !visible.contains("事情哦"),
        "预算之外的正文不得进入最终 UI 或 TTS"
    );
}
