use super::*;
use brain::TriggerKind;
use storage::{BrainActionRow, BrainTurnPreview, BrainTurnUpsert};

const MIN: i64 = 60_000;
const HOUR: i64 = 60 * MIN;
const DAY: i64 = 24 * HOUR;

fn turn(
    turn: i64,
    lane: &str,
    at: i64,
    kind: &str,
    who: &str,
    finished: Option<i64>,
) -> BrainTurnUpsert {
    BrainTurnUpsert {
        launch_id: 1,
        area_id: "a".into(),
        lane: lane.into(),
        turn,
        started_at_ms: at,
        trigger_kind: kind.into(),
        trigger_who: who.into(),
        finished_at_ms: finished,
        stop_reason: finished.map(|_| "end_turn".into()),
    }
}

fn preview(turn: i64, lane: &str, summary: &str) -> BrainTurnPreview {
    BrainTurnPreview {
        launch_id: 1,
        area_id: "a".into(),
        lane: lane.into(),
        turn,
        counts: vec![("prompt".into(), 1)],
        summary: summary.into(),
    }
}

fn act(at: i64, kind: &str, via: Option<(&str, i64)>) -> BrainActionRow {
    BrainActionRow {
        launch_id: 1,
        area_id: "a".into(),
        at_ms: at,
        kind: kind.into(),
        agent: "Chloe".into(),
        node_id: 3,
        summary: "跑测试".into(),
        reason: "只读".into(),
        ask_key: "assign".into(),
        via_lane: via.map(|(l, _)| l.into()),
        via_turn: via.map(|(_, t)| t),
    }
}

fn sample() -> Vec<FeedRow> {
    build_feed(
        vec![
            turn(1, "Main", 1000, "agent_finished", "Alan", Some(1500)),
            turn(
                2,
                "Approval",
                3000,
                "approval_request",
                "Claude",
                Some(3100),
            ),
            turn(3, "Approval", 5000, "approval_request", "Alan", Some(5100)),
        ],
        vec![
            preview(1, "Main", "完成"),
            preview(2, "Approval", "放行"),
            preview(3, "Approval", "拿不准"),
        ],
        vec![
            act(1200, "assigned", Some(("Main", 1))),
            act(3050, "approved", Some(("Approval", 2))),
            act(4000, "user", None),
        ],
    )
}

fn turn_numbers(rows: &[&FeedRow]) -> Vec<i64> {
    rows.iter()
        .filter_map(|r| match r {
            FeedRow::Turn(t) => Some(t.key.3),
            FeedRow::Action(_) => None,
        })
        .collect()
}

/// 轮次与动作按时间倒序合并成一条时间线，并带上互相的关系。
///
/// 数据构造：3 个轮次（第 3 轮是审批会话、没有产生动作）与 3 个动作（两个有来源轮次，一个是用户批准的）。
/// 预期结果：
///   - 正断言：时间顺序为 第3轮(5000)、用户动作(4000)、第2轮(3000)；第 1 轮产生 1 个动作；
///     动作的来源指向 (1,"a","Main",1)；摘要与条数来自预览
///   - 负断言：用户批准的动作没有来源；已有动作的审批轮次不算“需要你”
#[test]
fn feed_merges_turns_and_actions_newest_first() {
    let rows = sample();
    let order: Vec<i64> = rows.iter().map(FeedRow::at_ms).collect();
    assert_eq!(order, vec![5000, 4000, 3050, 3000, 1200, 1000]);
    let FeedRow::Turn(first) = &rows[5] else {
        panic!("最后一行应是第 1 轮")
    };
    assert_eq!(first.produced, 1);
    assert_eq!(first.summary, "完成");
    assert_eq!(first.chips, vec![("prompt".to_string(), 1)]);
    let FeedRow::Action(assigned) = &rows[4] else {
        panic!("动作")
    };
    assert_eq!(assigned.source, Some((1, "a".into(), "Main".into(), 1)));
    let FeedRow::Action(user) = &rows[1] else {
        panic!("动作")
    };
    assert_eq!(user.source, None);
    let FeedRow::Turn(approved) = &rows[3] else {
        panic!("第 2 轮")
    };
    assert!(!approved.needs_you);
}

/// “需要你”：已结束、没有产生任何动作的审批会话轮次；主会话轮次和仍在进行的不算。
#[test]
fn needs_you_means_a_finished_approval_turn_with_no_resulting_action() {
    let rows = sample();
    let needs: Vec<i64> = rows
        .iter()
        .filter_map(|r| match r {
            FeedRow::Turn(t) if t.needs_you => Some(t.key.3),
            _ => None,
        })
        .collect();
    assert_eq!(needs, vec![3]);
    let running = build_feed(
        vec![turn(9, "Approval", 1, "approval_request", "A", None)],
        vec![],
        vec![],
    );
    assert!(matches!(&running[0], FeedRow::Turn(t) if !t.needs_you));
}

/// 动作的触发源：有来源轮次时沿用该轮的触发源，用户批准的是“你批准提议”。
#[test]
fn an_action_inherits_the_trigger_of_its_source_turn() {
    let rows = sample();
    let FeedRow::Action(a) = &rows[4] else {
        panic!("动作")
    };
    assert_eq!(a.trigger, FeedTrigger::Brain(TriggerKind::AgentFinished));
    assert_eq!(a.trigger_who, "Alan");
    let FeedRow::Action(u) = &rows[1] else {
        panic!("动作")
    };
    assert_eq!(u.trigger, FeedTrigger::UserApproved);
}

/// 筛选：类型、项目、触发源可叠加；计数不随筛选变化。
#[test]
fn filters_combine_and_counts_ignore_them() {
    let rows = sample();
    let all = FeedFilter::default();
    assert_eq!(apply_filter(&rows, &all).len(), 6);
    assert_eq!(
        feed_counts(&rows),
        FeedCounts {
            all: 6,
            actions: 3,
            turns: 3,
            needs: 1
        }
    );
    let actions = FeedFilter {
        kind: FeedKind::Actions,
        ..Default::default()
    };
    assert_eq!(apply_filter(&rows, &actions).len(), 3);
    let needs = FeedFilter {
        kind: FeedKind::Needs,
        ..Default::default()
    };
    assert_eq!(turn_numbers(&apply_filter(&rows, &needs)), vec![3]);
    let approval = FeedFilter {
        trigger: Some(FeedTrigger::Brain(TriggerKind::ApprovalRequest)),
        ..Default::default()
    };
    assert_eq!(
        apply_filter(&rows, &approval).len(),
        3,
        "两个审批轮次，加上沿用审批触发源的那个放行动作"
    );
    let elsewhere = FeedFilter {
        area: Some("zzz".into()),
        ..Default::default()
    };
    assert!(apply_filter(&rows, &elsewhere).is_empty());
    assert_eq!(feed_areas(&rows), vec!["a".to_string()]);
    assert_eq!(
        feed_triggers(&rows),
        vec![
            FeedTrigger::Brain(TriggerKind::ApprovalRequest),
            FeedTrigger::UserApproved,
            FeedTrigger::Brain(TriggerKind::AgentFinished),
        ]
    );
}

/// 点“来自第 N 轮”要能在列表里找到那一轮；找不到（已被清理或不在当前页）返回空。
#[test]
fn the_source_turn_is_found_in_the_list() {
    let rows = sample();
    let FeedRow::Action(a) = &rows[4] else {
        panic!("动作")
    };
    let at = find_turn(&rows, a.source.as_ref().expect("有来源")).expect("在列表里");
    assert!(matches!(&rows[at], FeedRow::Turn(t) if t.key.3 == 1));
    assert_eq!(find_turn(&rows, &(1, "a".into(), "Main".into(), 99)), None);
}

/// 相对时间：1 分钟内“刚刚”，其后分钟、小时、天，不再一律“稍早”。
#[test]
fn relative_time_is_graded() {
    let now = 10 * DAY;
    assert_eq!(relative(now, now - 59_000), Relative::JustNow);
    assert_eq!(relative(now, now - 3 * MIN), Relative::Minutes(3));
    assert_eq!(relative(now, now - 59 * MIN), Relative::Minutes(59));
    assert_eq!(relative(now, now - 2 * HOUR - 5 * MIN), Relative::Hours(2));
    assert_eq!(relative(now, now - 3 * DAY), Relative::Days(3));
    assert_eq!(
        relative(now, now + MIN),
        Relative::JustNow,
        "时钟回拨也不出负数"
    );
}

/// 绝对时刻与按天分组都按本地时区；跨午夜时“今天/昨天”随时区偏移变化。
///
/// 数据构造：UTC 2026-10-25 15:30:08；偏移 +8 小时即本地 23:30:08，偏移 +9 小时即次日 00:30:08。
#[test]
fn clock_and_day_buckets_follow_the_local_offset() {
    let utc = 20_751 * DAY + 15 * HOUR + 30 * MIN + 8_000; // 20751 = 2026-10-25
    let plus8 = 8 * 3600;
    let plus9 = 9 * 3600;
    assert_eq!(clock(utc, plus8), "23:30:08");
    assert_eq!(clock(utc, plus9), "00:30:08");
    assert_eq!(day_bucket(utc, utc, plus8), DayBucket::Today);
    let yesterday_noon = utc - DAY;
    assert_eq!(day_bucket(utc, yesterday_noon, plus8), DayBucket::Yesterday);
    assert_eq!(
        day_bucket(utc, utc - 3 * DAY, plus8),
        DayBucket::Date { month: 10, day: 22 }
    );
    assert_eq!(
        day_bucket(utc, utc - DAY, plus9),
        DayBucket::Yesterday,
        "偏移 +9 时 now 已是 26 日，24 小时前是 25 日"
    );
}

/// 会话创建等事件落在“第 0 轮”：它不是一轮对话，不显示轮次号、用时和“进行中”。
///
/// 数据构造：第 0 轮（生命周期事件，没有结束时间）与正常的第 1 轮。
/// 预期结果：第 0 轮是会话事件；第 1 轮不是。
#[test]
fn turn_zero_is_a_session_event_not_a_numbered_turn() {
    let rows = build_feed(
        vec![
            turn(0, "Main", 1000, "unknown", "", None),
            turn(1, "Main", 2000, "agent_finished", "Alan", Some(2100)),
        ],
        vec![],
        vec![],
    );
    let flags: Vec<(i64, bool)> = rows
        .iter()
        .filter_map(|r| match r {
            FeedRow::Turn(t) => Some((t.key.3, t.is_session_event())),
            FeedRow::Action(_) => None,
        })
        .collect();
    assert_eq!(flags, vec![(1, false), (0, true)]);
}
