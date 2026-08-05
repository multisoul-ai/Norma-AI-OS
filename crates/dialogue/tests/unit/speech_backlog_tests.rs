use super::{SpeechBacklog, SpeechBacklogError, MAX_ACTIVE_SPEECH_CHARS};

/// BACKLOG-1：已经被 TTS worker 确认消费的字符必须释放 active 容量，累计朗读不得被 10,000 限制。
///
/// 数据构造（含关键数值的推导过程）：
///   full segments       = 166 × 60 + 40 = 10,000 active characters
///   acknowledged first  = 60 characters
///   active after ack     = 10,000 - 60 = 9,940 characters
///   replacement         = 60 characters
///   cumulative accepted = 10,000 + 60 = 10,060 characters
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 原子接受恰好 10,000 字的 segment batch → active 达到容量上限。
///   2. 提交并确认消费首个 60 字 segment → active 释放为 9,940。
///   3. 再接受一个 60 字 segment → active 回到 10,000，累计接受达到 10,060。
///
/// 预期结果：
///   - 正断言：确认消费会释放准确的 60 字 active 容量。
///   - 正断言：释放后可以接受具体 replacement segment，累计字符超过 10,000。
///   - 负断言：容量复用不得产生 capacity exceeded，也不得改变 pending 顺序。
#[test]
fn test_backlog_reuses_active_capacity_after_exact_acknowledgement() {
    let mut segments = vec!["甲".repeat(60); 166];
    segments.push("乙".repeat(40));
    let mut backlog = SpeechBacklog::default();

    backlog
        .accept_batch(segments)
        .expect("恰好 10,000 active characters 的 batch 必须被接受");
    assert_eq!(
        backlog.active_chars(),
        MAX_ACTIVE_SPEECH_CHARS,
        "166 × 60 + 40 必须恰好填满 10,000 字 active 容量"
    );
    assert_eq!(
        backlog.cumulative_accepted_chars(),
        10_000,
        "首次 batch 的累计接受字符必须为 10,000"
    );

    let first = backlog
        .next_submission()
        .expect("满容量 backlog 必须暴露首个待提交 segment");
    assert_eq!(
        first.text(),
        "甲".repeat(60),
        "首个待提交 segment 必须保持原始 60 字内容和顺序"
    );
    let first_receipt = first.receipt();
    backlog
        .mark_submitted(first_receipt)
        .expect("首个待提交 segment 必须能进入 submitted 状态");
    assert_eq!(
        backlog.active_chars(),
        10_000,
        "仅进入 command channel 尚未消费时不得提前释放 active 容量"
    );

    backlog
        .acknowledge_consumed(first_receipt)
        .expect("worker 必须能确认消费最早的 in-flight segment");
    assert_eq!(
        backlog.active_chars(),
        9_940,
        "确认消费 60 字后 active 必须由 10,000 精确降为 9,940"
    );
    assert_eq!(
        backlog.cumulative_consumed_chars(),
        60,
        "累计消费字符必须记录首个具体 60 字 segment"
    );

    backlog
        .accept_batch(vec!["丁".repeat(60)])
        .expect("释放 60 字后必须能接受同样长度的 replacement segment");
    assert_eq!(
        backlog.active_chars(),
        10_000,
        "接受 replacement 后 active 必须重新达到 10,000"
    );
    assert_eq!(
        backlog.cumulative_accepted_chars(),
        10_060,
        "容量复用后累计接受必须达到 10,060，不能形成整轮硬上限"
    );
    assert!(
        !backlog.capacity_exceeded(),
        "active 容量从未超过 10,000 时不得误报 capacity exceeded"
    );
    assert_eq!(
        backlog
            .next_submission()
            .expect("消费首段后仍应保留第二个原始 segment")
            .text(),
        "甲".repeat(60),
        "replacement 必须追加在原始 pending 尾部，不能越过第二个甲 segment"
    );
}

/// BACKLOG-2：超过 10,000 active characters 的 batch 必须原子拒绝并关闭本轮 speech input。
///
/// 数据构造（含关键数值的推导过程）：
///   accepted batch = 166 × 60 + 40 = 10,000 active characters
///   overflow batch = 1 character
///   attempted      = 10,000 + 1 = 10,001 active characters
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 接受恰好 10,000 字并保存首个具体 segment。
///   2. 再接受 1 字 batch → 返回 CapacityExceeded，且不部分写入。
///   3. 尝试追加另一段 → 因 speech input 已关闭而拒绝，原有 10,000 字保持不变。
///
/// 预期结果：
///   - 正断言：错误携带 active、incoming 和 capacity 三个准确数值。
///   - 正断言：原有首段、active 和累计接受字符保持不变。
///   - 负断言：overflow 字符不得进入队列，后续 batch 不得重新打开本轮输入。
#[test]
fn test_backlog_atomically_rejects_capacity_overflow_and_closes_input() {
    let mut accepted = vec!["甲".repeat(60); 166];
    accepted.push("尾".repeat(40));
    let mut backlog = SpeechBacklog::default();
    backlog
        .accept_batch(accepted)
        .expect("166 × 60 + 40 必须恰好填满 active 容量");

    let overflow = backlog
        .accept_batch(vec!["溢".to_string()])
        .expect_err("10,000 + 1 active characters 必须明确拒绝");

    assert_eq!(
        overflow,
        SpeechBacklogError::CapacityExceeded {
            active_chars: 10_000,
            incoming_chars: 1,
            capacity_chars: 10_000,
        },
        "capacity error 必须暴露 10,000 + 1 的精确推导，不能伪装为 Provider 错误"
    );
    assert!(
        backlog.capacity_exceeded(),
        "原子拒绝后必须记录本轮 speech capacity 已超限"
    );
    assert_eq!(
        backlog.active_chars(),
        10_000,
        "overflow batch 不得部分进入，active 必须仍为原有 10,000"
    );
    assert_eq!(
        backlog.cumulative_accepted_chars(),
        10_000,
        "被拒绝的 1 字不得污染累计接受字符"
    );
    assert_eq!(
        backlog
            .next_submission()
            .expect("capacity overflow 后原有首段必须保留")
            .text(),
        "甲".repeat(60),
        "capacity overflow 不得清空或重排已接受 speech"
    );
    assert_eq!(
        backlog
            .accept_batch(vec!["后续内容".to_string()])
            .expect_err("capacity overflow 后本轮 speech input 必须保持关闭"),
        SpeechBacklogError::InputClosed,
        "容量超限后的后续 delta 不得重新打开 speech input"
    );
}

/// BACKLOG-3：worker acknowledgement 必须严格匹配最早的 submitted segment。
///
/// 数据构造（含关键数值的推导过程）：
///   first segment  = 60 characters
///   second segment = 60 characters
///   active total   = 60 + 60 = 120 characters
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 顺序接受并提交 first、second 两段。
///   2. 先用 second receipt 确认 → 拒绝乱序确认且不释放容量。
///   3. 依次确认 first、second → active 从 120 降至 60，再降至 0。
///
/// 预期结果：
///   - 正断言：正确顺序的两次确认分别释放准确的 60 字。
///   - 负断言：乱序确认不得增加 consumed 或提前使 backlog drained。
///   - 正断言：关闭输入且全部确认后 backlog 才可 finish。
#[test]
fn test_backlog_rejects_out_of_order_acknowledgement_without_releasing_capacity() {
    let mut backlog = SpeechBacklog::default();
    backlog
        .accept_batch(vec!["甲".repeat(60), "乙".repeat(60)])
        .expect("两段共 120 字必须进入 backlog");
    let first = backlog
        .next_submission()
        .expect("第一段必须可提交")
        .receipt();
    backlog
        .mark_submitted(first)
        .expect("第一段 receipt 必须匹配 pending front");
    let second = backlog
        .next_submission()
        .expect("第一段提交后第二段必须成为 pending front")
        .receipt();
    backlog
        .mark_submitted(second)
        .expect("第二段 receipt 必须匹配新的 pending front");

    assert_eq!(
        backlog
            .acknowledge_consumed(second)
            .expect_err("第二段不得越过第一段被确认"),
        SpeechBacklogError::AcknowledgementMismatch,
        "乱序 acknowledgement 必须返回具体 mismatch"
    );
    assert_eq!(
        backlog.active_chars(),
        120,
        "乱序 acknowledgement 不得释放任何 active 字符"
    );
    assert_eq!(
        backlog.cumulative_consumed_chars(),
        0,
        "乱序 acknowledgement 不得计入 consumed 诊断"
    );

    backlog
        .acknowledge_consumed(first)
        .expect("最早 first receipt 必须能被确认");
    assert_eq!(
        backlog.active_chars(),
        60,
        "确认 first 后必须只释放 first 的 60 字"
    );
    backlog
        .acknowledge_consumed(second)
        .expect("first 完成后 second receipt 必须能被确认");
    backlog.close_input();
    assert_eq!(
        backlog.active_chars(),
        0,
        "两段均确认后 active 必须精确归零"
    );
    assert_eq!(
        backlog.cumulative_consumed_chars(),
        120,
        "两段正确确认后累计 consumed 必须为 60 + 60 = 120"
    );
    assert!(
        backlog.ready_to_finish(),
        "input closed 且 pending/submitted 全空后必须允许 finish"
    );
}
