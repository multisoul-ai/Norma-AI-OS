use super::*;
use brain::acp::trace::{AcpTrace, TraceItem, TraceKind};
use std::time::{Duration, SystemTime};

fn at(secs: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
}

fn trace(lane: &str, turn: u64, secs: u64, text: &str) -> AcpTrace {
    AcpTrace {
        at: at(secs),
        lane: lane.into(),
        turn,
        item: TraceItem::new(TraceKind::Reply, "", text),
    }
}

/// 缓冲按条数封顶，超出时丢最旧的，保留最新的。
///
/// 数据构造：连续放入 MAX_ITEMS + 5 条，正文是序号。
/// 预期结果：
///   - 正断言：只剩 MAX_ITEMS 条，第一条是序号 5，最后一条是最新
///   - 负断言：序号 0 到 4 已被丢弃
#[test]
fn buffer_keeps_only_the_newest_items() {
    let mut buffer = TraceBuffer::default();
    for n in 0..(MAX_ITEMS + 5) {
        buffer.push(trace("Main", 1, n as u64, &n.to_string()));
    }
    let items: Vec<&AcpTrace> = buffer.items().collect();
    assert_eq!(items.len(), MAX_ITEMS);
    assert_eq!(items[0].item.text, "5");
    assert_eq!(items.last().unwrap().item.text, (MAX_ITEMS + 4).to_string());
}

/// 缓冲也按总字数封顶，一条超大的记录不能把内存吃光。
///
/// 数据构造：每条 1MB 的正文，放入 4 条。
/// 预期结果：
///   - 正断言：总字节数不超过 MAX_BYTES 加一条的余量，最新一条一定保留
///   - 负断言：最早的几条被丢弃
#[test]
fn buffer_is_capped_by_total_bytes() {
    let mut buffer = TraceBuffer::default();
    let big = "x".repeat(1024 * 1024);
    for n in 0..4 {
        buffer.push(trace("Main", n, n, &big));
    }
    assert!(buffer.items().count() <= 2);
    assert_eq!(buffer.items().last().unwrap().turn, 3);
}

/// 同一轮的记录归到一张卡片；不同项目、不同会话、不同轮次不合并；新的在前。
///
/// 数据构造：项目 a 的 Main 第 1 轮两条，项目 b 的 Main 第 1 轮一条，项目 a 的 Approval 第 1 轮一条，项目 a 的 Main 第 2 轮一条。
/// 预期结果：
///   - 正断言：共 4 张卡；最新开始的在最前；a 的 Main 第 1 轮卡里有两条且保持原顺序
///   - 负断言：a 与 b 的 Main 第 1 轮没有合并
#[test]
fn turns_group_by_project_lane_and_turn() {
    let entries = vec![
        ("a".to_string(), trace("Main", 1, 10, "一")),
        ("a".to_string(), trace("Main", 1, 11, "二")),
        ("b".to_string(), trace("Main", 1, 12, "乙")),
        ("a".to_string(), trace("Approval", 1, 13, "审")),
        ("a".to_string(), trace("Main", 2, 20, "新")),
    ];
    let cards = group_turns(&entries);
    assert_eq!(cards.len(), 4);
    assert_eq!(cards[0].turn, 2, "最新开始的在最前");
    let first = cards
        .iter()
        .find(|c| c.area == "a" && c.lane == "Main" && c.turn == 1)
        .expect("a 的 Main 第 1 轮");
    let texts: Vec<&str> = first.items.iter().map(|i| i.text.as_str()).collect();
    assert_eq!(texts, vec!["一", "二"]);
    assert!(cards.iter().any(|c| c.area == "b"));
}
