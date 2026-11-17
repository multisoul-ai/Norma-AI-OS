use agent::AgentRosterEntry;

/// 先挑不忙的，再挑画布上同名卡最少的；并列时保持名册顺序。
pub(super) fn least_loaded<'a>(
    found: &mut [&'a AgentRosterEntry],
    load: impl Fn(&AgentRosterEntry) -> usize,
) -> Option<&'a AgentRosterEntry> {
    found.sort_by_key(|entry| (entry.busy, load(entry)));
    found.first().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, busy: bool) -> AgentRosterEntry {
        AgentRosterEntry {
            definition_id: 0,
            name: name.into(),
            speech_alias: None,
            profile: norma::protocol::AgentProfile::Claude,
            enabled: true,
            active_node_id: None,
            tracking: agent::AgentTrackingMode::Managed,
            busy,
            task_state: agent::AgentTaskState::Ready,
        }
    }

    /// [场景描述]：同一 engine 连续派多个活时，名字应轮换而不是总落在第一个上。
    ///
    /// 数据构造：Alan、Bella 均空闲；画布上 Alan 已有 1 张幕僚卡、Bella 有 0 张。
    ///
    /// 执行过程：
    ///   1. busy 都为 false，持平 → 比较同名卡数：Alan=1、Bella=0
    ///   2. 选卡数少的 Bella
    ///
    /// 预期结果：
    ///   - 选中 Bella：Alan 已占一张卡，不应再被重复选中
    ///   - 若 Bella 忙而 Alan 空闲则仍选 Alan：busy 优先于卡数
    #[test]
    fn spreads_across_names_by_card_count() {
        let (alan, bella) = (entry("Alan", false), entry("Bella", false));
        let load = |e: &AgentRosterEntry| usize::from(e.name == "Alan");
        let picked = least_loaded(&mut [&alan, &bella], load).unwrap();
        assert_eq!(
            picked.name, "Bella",
            "Alan already has a card; Bella must be chosen"
        );

        let busy_bella = entry("Bella", true);
        let picked = least_loaded(&mut [&alan, &busy_bella], load).unwrap();
        assert_eq!(picked.name, "Alan", "busy outranks card count");
    }
}
