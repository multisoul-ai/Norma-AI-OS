use agent::AgentRosterEntry;

/// 一张可以复用的卡。用户自己开的卡只有进程身份能认领时才算。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CardCandidate {
    pub node: u64,
    pub name: String,
    pub busy: bool,
    pub user_opened: bool,
    pub claimable: bool,
}

/// 点名时优先用同名空闲卡。正忙，或用户开的卡认领不了，就不当成可复用。
pub(super) fn reusable_card<'a>(
    name: &str,
    cards: &'a [CardCandidate],
) -> Option<&'a CardCandidate> {
    cards.iter().find(|card| {
        card.name.eq_ignore_ascii_case(name) && !card.busy && (!card.user_opened || card.claimable)
    })
}

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

    /// 场景：点名 Grace 时画布上已有她的卡。
    /// 数据构造：一张空闲的幕僚卡、一张正忙的同名卡、一张用户开的但没有进程身份的卡。
    /// 执行过程：按名字找可复用的卡。
    /// 预期结果：用空闲的那张；忙的和认领不了的都跳过。
    #[test]
    fn named_reuse_skips_busy_and_unclaimable_cards() {
        let cards = [
            CardCandidate {
                node: 1,
                name: "Grace".into(),
                busy: true,
                user_opened: false,
                claimable: true,
            },
            CardCandidate {
                node: 2,
                name: "Grace".into(),
                busy: false,
                user_opened: true,
                claimable: false,
            },
            CardCandidate {
                node: 3,
                name: "Grace".into(),
                busy: false,
                user_opened: false,
                claimable: true,
            },
        ];
        let picked = reusable_card("grace", &cards).expect("idle card");
        assert_eq!(picked.node, 3, "空闲的幕僚卡可以复用");
        assert!(reusable_card("Alan", &cards).is_none(), "没有同名卡就新开");
        let only_busy = [cards[0].clone()];
        assert!(
            reusable_card("Grace", &only_busy).is_none(),
            "正忙就新开，不抢正在跑的卡"
        );
    }

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
