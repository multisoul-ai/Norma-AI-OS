use super::*;

/// 事件队列达到上限后保持有界，并用 EventsDropped 标记丢弃。
///
/// 数据构造（含关键数值推导）：
///   MAX_PENDING_EVENTS = 1024
///   写入事件数 = MAX_PENDING_EVENTS + 1 = 1025
///   第 1025 条写入时队列已满，因此 pop_front 1 条并写入 EventsDropped{count:1}
///
/// 执行过程：
///   1. RuntimeEvents::default() → queue.len=0
///   2. 连续 register 1025 次，每次最多追加 1 个事件
///   3. 第 1025 次 register 触发上限保护，丢弃最旧事件
///   4. take() 消费全部 pending events
///
/// 预期结果：
///   - 正断言：pending_len 保持为 MAX_PENDING_EVENTS，不无限增长
///   - 正断言：消费结果包含 EventsDropped{count:1}
///   - 负断言：消费结果不应有 MAX_PENDING_EVENTS + 1 条事件
#[test]
fn test_event_queue_emits_drop_marker_when_capacity_is_exceeded() {
    let mut events = RuntimeEvents::default();

    for node_id in 1..=(MAX_PENDING_EVENTS as NodeId + 1) {
        events.register(node_id, NodeType::Term);
    }

    assert_eq!(
        events.pending_len(),
        MAX_PENDING_EVENTS,
        "超过上限后 pending_len 应仍保持 MAX_PENDING_EVENTS"
    );
    let taken = events.take();
    assert!(
        taken
            .iter()
            .any(|event| matches!(event, RuntimeEvent::EventsDropped { count: 1 })),
        "超过队列上限时应产生 EventsDropped{{count:1}}，实际: {:?}",
        taken
    );
    assert_ne!(
        taken.len(),
        MAX_PENDING_EVENTS + 1,
        "队列不应增长到 MAX_PENDING_EVENTS + 1"
    );
}
