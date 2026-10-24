//! 大脑活动窗口的数据：有上限的记录缓冲，以及把记录按“轮”分组。只放内存。

use brain::acp::trace::{AcpTrace, TraceItem};
use std::collections::VecDeque;
use std::time::SystemTime;

pub(super) const MAX_ITEMS: usize = 2000;
pub(super) const MAX_BYTES: usize = 2 * 1024 * 1024;

fn size_of(trace: &AcpTrace) -> usize {
    trace.item.title.len() + trace.item.text.len() + trace.lane.len() + 32
}

/// 最近的 ACP 记录；超过条数或总字节上限时丢最旧的。
#[derive(Debug, Default)]
pub(crate) struct TraceBuffer {
    items: VecDeque<AcpTrace>,
    bytes: usize,
}

impl TraceBuffer {
    pub(crate) fn push(&mut self, trace: AcpTrace) {
        self.bytes += size_of(&trace);
        self.items.push_back(trace);
        while self.items.len() > 1 && (self.items.len() > MAX_ITEMS || self.bytes > MAX_BYTES) {
            if let Some(old) = self.items.pop_front() {
                self.bytes -= size_of(&old);
            }
        }
    }

    pub(crate) fn items(&self) -> impl Iterator<Item = &AcpTrace> {
        self.items.iter()
    }
}

/// 窗口里的一张卡片：某个项目、某条会话的一轮。
#[derive(Clone, Debug)]
pub(crate) struct TurnCard {
    pub area: String,
    pub lane: String,
    pub turn: u64,
    pub at: SystemTime,
    pub items: Vec<TraceItem>,
}

/// 按（项目，会话，轮次）分组，保持每组内的原顺序；开始得晚的卡片在前。
pub(crate) fn group_turns(entries: &[(String, AcpTrace)]) -> Vec<TurnCard> {
    let mut cards: Vec<TurnCard> = Vec::new();
    for (area, trace) in entries {
        let existing = cards
            .iter_mut()
            .find(|c| c.area == *area && c.lane == trace.lane && c.turn == trace.turn);
        match existing {
            Some(card) => card.items.push(trace.item.clone()),
            None => cards.push(TurnCard {
                area: area.clone(),
                lane: trace.lane.clone(),
                turn: trace.turn,
                at: trace.at,
                items: vec![trace.item.clone()],
            }),
        }
    }
    cards.sort_by(|a, b| b.at.cmp(&a.at));
    cards
}

#[cfg(test)]
#[path = "../../../tests/unit/brain_trace_tests.rs"]
mod brain_trace_tests;
