//! 幕僚活动面板的数据：把持久化的轮次与动作合并成一条时间线，附带筛选与时间格式化。纯函数，不碰界面与数据库。

use brain::TriggerKind;
use storage::{BrainActionRow, BrainTurnPreview, BrainTurnUpsert};

/// （启动标识，项目区域，会话，轮次）：轮次号每次启动从头计数，所以带上启动标识。
pub(crate) type TurnKey = (i64, String, String, i64);

/// 触发源：大脑自己的触发事件，或用户在概览里批准了提议。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FeedTrigger {
    Brain(TriggerKind),
    UserApproved,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FeedTurn {
    pub key: TurnKey,
    pub started_ms: i64,
    pub finished_ms: Option<i64>,
    pub stop_reason: Option<String>,
    pub trigger: FeedTrigger,
    pub trigger_who: String,
    pub chips: Vec<(String, usize)>,
    pub summary: String,
    /// 本轮产生的动作数。
    pub produced: usize,
    /// 已结束的审批会话轮次、却没有产生任何放行或拒绝：交给了用户。
    pub needs_you: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FeedAction {
    pub row: BrainActionRow,
    pub source: Option<TurnKey>,
    pub trigger: FeedTrigger,
    pub trigger_who: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FeedRow {
    Turn(FeedTurn),
    Action(FeedAction),
}

impl FeedRow {
    pub(crate) fn at_ms(&self) -> i64 {
        match self {
            Self::Turn(turn) => turn.started_ms,
            Self::Action(action) => action.row.at_ms,
        }
    }

    pub(crate) fn area(&self) -> &str {
        match self {
            Self::Turn(turn) => &turn.key.1,
            Self::Action(action) => &action.row.area_id,
        }
    }

    pub(crate) fn trigger(&self) -> FeedTrigger {
        match self {
            Self::Turn(turn) => turn.trigger,
            Self::Action(action) => action.trigger,
        }
    }
}

/// 合并并按时间倒序排列；同一时刻动作排在轮次前面（动作发生在轮次之中）。
pub(crate) fn build_feed(
    turns: Vec<BrainTurnUpsert>,
    previews: Vec<BrainTurnPreview>,
    actions: Vec<BrainActionRow>,
) -> Vec<FeedRow> {
    let source_of = |a: &BrainActionRow| -> Option<TurnKey> {
        Some((
            a.launch_id,
            a.area_id.clone(),
            a.via_lane.clone()?,
            a.via_turn?,
        ))
    };
    let produced = |key: &TurnKey| {
        actions
            .iter()
            .filter(|a| source_of(a).as_ref() == Some(key))
            .count()
    };
    let mut rows: Vec<FeedRow> = Vec::with_capacity(turns.len() + actions.len());
    for t in &turns {
        let key: TurnKey = (t.launch_id, t.area_id.clone(), t.lane.clone(), t.turn);
        let preview = previews
            .iter()
            .find(|p| (p.launch_id, &p.area_id, &p.lane, p.turn) == (key.0, &key.1, &key.2, key.3));
        let produced = produced(&key);
        rows.push(FeedRow::Turn(FeedTurn {
            started_ms: t.started_at_ms,
            finished_ms: t.finished_at_ms,
            stop_reason: t.stop_reason.clone(),
            trigger: FeedTrigger::Brain(TriggerKind::parse(&t.trigger_kind)),
            trigger_who: t.trigger_who.clone(),
            chips: preview.map(|p| p.counts.clone()).unwrap_or_default(),
            summary: preview.map(|p| p.summary.clone()).unwrap_or_default(),
            needs_you: key.2 == "Approval" && t.finished_at_ms.is_some() && produced == 0,
            produced,
            key,
        }));
    }
    for a in actions.iter() {
        let source = source_of(a);
        let origin = source.as_ref().and_then(|key| {
            turns.iter().find(|t| {
                (t.launch_id, &t.area_id, &t.lane, t.turn) == (key.0, &key.1, &key.2, key.3)
            })
        });
        let (trigger, who) = match (a.kind.as_str(), origin) {
            ("user", _) => (FeedTrigger::UserApproved, String::new()),
            (_, Some(t)) => (
                FeedTrigger::Brain(TriggerKind::parse(&t.trigger_kind)),
                t.trigger_who.clone(),
            ),
            _ => (FeedTrigger::Brain(TriggerKind::Unknown), String::new()),
        };
        rows.push(FeedRow::Action(FeedAction {
            row: a.clone(),
            source,
            trigger,
            trigger_who: who,
        }));
    }
    rows.sort_by(|a, b| {
        b.at_ms()
            .cmp(&a.at_ms())
            .then_with(|| matches!(a, FeedRow::Turn(_)).cmp(&matches!(b, FeedRow::Turn(_))))
    });
    rows
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum FeedKind {
    #[default]
    All,
    Actions,
    Turns,
    Needs,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct FeedFilter {
    pub kind: FeedKind,
    pub area: Option<String>,
    pub trigger: Option<FeedTrigger>,
}

pub(crate) fn apply_filter<'a>(rows: &'a [FeedRow], filter: &FeedFilter) -> Vec<&'a FeedRow> {
    rows.iter()
        .filter(|row| match (filter.kind, row) {
            (FeedKind::All, _) => true,
            (FeedKind::Actions, FeedRow::Action(_)) | (FeedKind::Turns, FeedRow::Turn(_)) => true,
            (FeedKind::Needs, FeedRow::Turn(turn)) => turn.needs_you,
            _ => false,
        })
        .filter(|row| filter.area.as_deref().is_none_or(|area| row.area() == area))
        .filter(|row| {
            filter
                .trigger
                .is_none_or(|trigger| row.trigger() == trigger)
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FeedCounts {
    pub all: usize,
    pub actions: usize,
    pub turns: usize,
    pub needs: usize,
}

/// 各类型的条数，不受当前筛选影响。
pub(crate) fn feed_counts(rows: &[FeedRow]) -> FeedCounts {
    let actions = rows
        .iter()
        .filter(|r| matches!(r, FeedRow::Action(_)))
        .count();
    FeedCounts {
        all: rows.len(),
        actions,
        turns: rows.len() - actions,
        needs: rows
            .iter()
            .filter(|r| matches!(r, FeedRow::Turn(t) if t.needs_you))
            .count(),
    }
}

/// 列表里出现过的项目区域，按首次出现的顺序。
pub(crate) fn feed_areas(rows: &[FeedRow]) -> Vec<String> {
    let mut areas: Vec<String> = Vec::new();
    for row in rows {
        if !areas.iter().any(|a| a == row.area()) {
            areas.push(row.area().to_owned());
        }
    }
    areas
}

/// 列表里出现过的触发源，按首次出现的顺序。
pub(crate) fn feed_triggers(rows: &[FeedRow]) -> Vec<FeedTrigger> {
    let mut triggers: Vec<FeedTrigger> = Vec::new();
    for row in rows {
        if !triggers.contains(&row.trigger()) {
            triggers.push(row.trigger());
        }
    }
    triggers
}

/// 某一轮在列表里的位置；已被清理或不在当前页时为空。
pub(crate) fn find_turn(rows: &[FeedRow], key: &TurnKey) -> Option<usize> {
    rows.iter()
        .position(|row| matches!(row, FeedRow::Turn(turn) if &turn.key == key))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Relative {
    JustNow,
    Minutes(u64),
    Hours(u64),
    Days(u64),
}

pub(crate) fn relative(now_ms: i64, at_ms: i64) -> Relative {
    let seconds = ((now_ms - at_ms).max(0) / 1000) as u64;
    match seconds {
        0..=59 => Relative::JustNow,
        60..=3599 => Relative::Minutes(seconds / 60),
        3600..=86_399 => Relative::Hours(seconds / 3600),
        _ => Relative::Days(seconds / 86_400),
    }
}

/// 本地时区的 `HH:MM:SS`。
pub(crate) fn clock(at_ms: i64, offset_secs: i64) -> String {
    let secs = (at_ms / 1000 + offset_secs).rem_euclid(86_400);
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DayBucket {
    Today,
    Yesterday,
    Date { month: u32, day: u32 },
}

pub(crate) fn day_bucket(now_ms: i64, at_ms: i64, offset_secs: i64) -> DayBucket {
    let day_of = |ms: i64| (ms / 1000 + offset_secs).div_euclid(86_400);
    match day_of(now_ms) - day_of(at_ms) {
        ..=0 => DayBucket::Today,
        1 => DayBucket::Yesterday,
        _ => {
            let (month, day) = civil_month_day(day_of(at_ms));
            DayBucket::Date { month, day }
        }
    }
}

/// 自 1970-01-01 起的天数 → （月，日）。Howard Hinnant 的 civil_from_days。
fn civil_month_day(days: i64) -> (u32, u32) {
    let z = days + 719_468;
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (month, day)
}

/// 本机时区相对 UTC 的偏移（秒）。
pub(crate) fn local_offset_secs(at_ms: i64) -> i64 {
    let time: libc::time_t = (at_ms / 1000) as libc::time_t;
    let mut parts: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: `localtime_r` 只写入我们提供的 `tm`，`time` 是有效的局部变量。
    let filled = unsafe { libc::localtime_r(&time, &mut parts) };
    if filled.is_null() {
        0
    } else {
        parts.tm_gmtoff
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/brain_activity_feed_tests.rs"]
mod brain_activity_feed_tests;
