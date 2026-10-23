use super::super::{AgentAnnouncementOutcome, PendingAgentAnnouncement};
use super::*;

fn pending(
    node_id: u64,
    name: &str,
    outcome: AgentAnnouncementOutcome,
    age_secs: u64,
    now: Instant,
) -> PendingAgentAnnouncement {
    PendingAgentAnnouncement {
        announcement: AgentAnnouncement::terminal(node_id, name, outcome),
        waiting_for_tts: false,
        deferred: true,
        queued_at: now - Duration::from_secs(age_secs),
    }
}

/// 通道重连期间积压的通知超过 60 秒即丢弃，不在恢复后补播。
///
/// 数据构造：now 固定；Chloe 入队 61 s（> TTL 60 s），Bob 入队 10 s（< 60 s）；
///   边界项 Mira 恰好 60 s，按「已满 TTL」丢弃。
/// 执行过程：调用 take_ready_batch(now)。
/// 预期结果：只播报 "Bob, Failed"；Chloe、Mira 被丢弃，队列清空。
#[test]
fn stale_announcements_are_discarded_before_release() {
    let now = Instant::now() + Duration::from_secs(120);
    let mut announcements = AgentAnnouncements::default();
    announcements.pending.extend([
        pending(1, "Chloe", AgentAnnouncementOutcome::Done, 61, now),
        pending(3, "Mira", AgentAnnouncementOutcome::Done, 60, now),
        pending(2, "Bob", AgentAnnouncementOutcome::Failed, 10, now),
    ]);
    let (announcement, queued_at) = announcements
        .take_ready_batch(now)
        .expect("未过期的通知应放行");
    assert_eq!(announcement.text, "Bob, Failed", "只应播报 60 秒内的通知");
    assert_eq!(
        queued_at,
        now - Duration::from_secs(10),
        "应保留原入队时间，打断后按原时效重排"
    );
    assert!(announcements.pending.is_empty(), "过期通知不能留在队列");
}

/// 通道恢复时积压的多条通知合成一句；同一卡片只保留最新结果；未就绪项截断批次。
///
/// 数据构造：积压且已就绪的 Chloe Done(node 1) → Bob Failed(node 2) → Chloe Failed(node 1)，
///   之后一条等待 TTS 的 Mira(node 3)。所有项入队 5 s。
/// 执行过程：调用 take_ready_batch(now)。
/// 预期结果：合成 "Bob, Failed; Chloe, Failed"；Mira 仍在队列等待；不出现 "Chloe, Done"。
#[test]
fn ready_announcements_merge_into_one_report() {
    let now = Instant::now() + Duration::from_secs(120);
    let mut announcements = AgentAnnouncements::default();
    let mut waiting = pending(3, "Mira", AgentAnnouncementOutcome::Done, 5, now);
    waiting.waiting_for_tts = true;
    announcements.pending.extend([
        pending(1, "Chloe", AgentAnnouncementOutcome::Done, 5, now),
        pending(2, "Bob", AgentAnnouncementOutcome::Failed, 5, now),
        pending(1, "Chloe", AgentAnnouncementOutcome::Failed, 5, now),
        waiting,
    ]);
    let (announcement, _) = announcements.take_ready_batch(now).expect("就绪通知应放行");
    assert_eq!(
        announcement.text, "Bob, Failed; Chloe, Failed",
        "就绪通知应合成一句，同卡片只保留最新结果"
    );
    assert!(
        !announcement.text.contains("Chloe, Done"),
        "同一卡片的旧结果不能再播"
    );
    assert_eq!(announcements.pending.len(), 1, "等待中的通知不能被合并放行");
    assert!(
        announcements.take_ready_batch(now).is_none(),
        "队首仍在等待 TTS 时不能放行"
    );
}

/// 通道正常时逐条播报：非积压通知不合并，保持每个回合的独立结果。
///
/// 数据构造：两条非积压就绪通知 Chloe Interrupted(node 1) 与 Chloe Done(node 1)，入队 1 s。
/// 执行过程：连续两次 take_ready_batch(now)。
/// 预期结果：依次得到 "Chloe, Interrupted" 与 "Chloe, Done"，不合并、不去重。
#[test]
fn live_announcements_stay_separate() {
    let now = Instant::now() + Duration::from_secs(120);
    let mut announcements = AgentAnnouncements::default();
    let live = |outcome| PendingAgentAnnouncement {
        deferred: false,
        ..pending(1, "Chloe", outcome, 1, now)
    };
    announcements.pending.extend([
        live(AgentAnnouncementOutcome::Interrupted),
        live(AgentAnnouncementOutcome::Done),
    ]);
    let first = announcements
        .take_ready_batch(now)
        .map(|(item, _)| item.text);
    let second = announcements
        .take_ready_batch(now)
        .map(|(item, _)| item.text);
    assert_eq!(
        first.as_deref(),
        Some("Chloe, Interrupted"),
        "非积压通知应逐条播报"
    );
    assert_eq!(
        second.as_deref(),
        Some("Chloe, Done"),
        "同一卡片的新回合结果不能被合并或去重"
    );
}
