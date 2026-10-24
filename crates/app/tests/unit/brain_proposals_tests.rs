use super::*;
use std::time::{Duration, SystemTime};

fn draft(agent: &str, title: &str) -> Draft {
    Draft {
        agent: agent.into(),
        title: title.into(),
        task: format!("{title}：完整指令"),
        why: "理由".into(),
    }
}

fn entry(name: &str, card: bool, busy: bool) -> RosterLite {
    RosterLite {
        name: name.into(),
        enabled: true,
        has_card: card,
        busy,
    }
}

/// 同一个 Agent 只留最新一条提议；总数超过上限时拒绝新的。
///
/// 数据构造：给 Alan 提两次（第二次标题不同），再给另外 4 个 Agent 各提一次，最后给第 6 个 Agent 提。
/// 预期结果：
///   - 正断言：Alan 只剩第二次的提议；前 5 个 Agent 各一条
///   - 负断言：第 6 条被拒绝，列表不超过 5 条
#[test]
fn same_agent_replaces_and_total_is_capped() {
    let mut proposals = Proposals::default();
    let at = SystemTime::UNIX_EPOCH;
    proposals
        .add(draft("Alan", "旧"), at)
        .expect("第一次应成功");
    proposals.add(draft("Alan", "新"), at).expect("覆盖应成功");
    let alan: Vec<_> = proposals
        .items()
        .iter()
        .filter(|p| p.agent == "Alan")
        .collect();
    assert_eq!(alan.len(), 1, "同一 Agent 只留一条");
    assert_eq!(alan[0].title, "新");
    for name in ["B", "C", "D", "E"] {
        proposals.add(draft(name, "x"), at).expect("未满应成功");
    }
    assert!(
        proposals.add(draft("F", "x"), at).is_err(),
        "超过 5 条应拒绝"
    );
    assert_eq!(proposals.items().len(), MAX_PROPOSALS);
}

/// 取出提议后它从列表消失，编号不复用。
///
/// 数据构造：提两条，取出第一条，再提一条。
/// 预期结果：
///   - 正断言：取出的就是第一条；新提议的编号大于之前所有编号
///   - 负断言：同一编号不能取两次
#[test]
fn take_removes_and_ids_are_not_reused() {
    let mut proposals = Proposals::default();
    let at = SystemTime::UNIX_EPOCH;
    let first = proposals.add(draft("A", "一"), at).unwrap();
    let second = proposals.add(draft("B", "二"), at).unwrap();
    assert_eq!(proposals.take(first).map(|p| p.title), Some("一".into()));
    assert!(proposals.take(first).is_none(), "不能取两次");
    let third = proposals.add(draft("C", "三"), at).unwrap();
    assert!(third > second, "编号不复用");
}

/// 写入再读回，内容与编号都保留；损坏的文件当作没有提议。
///
/// 数据构造：两条提议（时间不同）序列化后再解析；另给一段非 JSON 文本。
/// 预期结果：
///   - 正断言：读回的提议与原来逐字段相同，下一个编号接着往后
///   - 负断言：损坏的文本得到空列表，不会崩溃
#[test]
fn round_trips_through_json() {
    let mut proposals = Proposals::default();
    proposals
        .add(draft("A", "一"), SystemTime::UNIX_EPOCH)
        .unwrap();
    proposals
        .add(
            draft("B", "二"),
            SystemTime::UNIX_EPOCH + Duration::from_secs(90),
        )
        .unwrap();
    let mut back = Proposals::from_json(&proposals.to_json());
    assert_eq!(back.items(), proposals.items());
    let next = back.add(draft("C", "三"), SystemTime::UNIX_EPOCH).unwrap();
    assert!(next > 2, "编号接着往后：{next}");
    assert!(Proposals::from_json("not json").items().is_empty());
}

/// 提议的目标必须是存在且启用的 Agent；只有没有空闲卡片时才可以提议给还没有卡片的 Agent。
///
/// 数据构造：名册里有空闲卡片 Alan、忙碌卡片 Bob、没有卡片的 Cora、停用的 Dan。
/// 预期结果：
///   - 正断言：提给 Alan、Bob 都可以；Alan 也忙时，提给 Cora 可以
///   - 负断言：未知名字、停用的 Agent 被拒；有空闲卡片时提给 Cora 被拒，错误里点名 Alan
#[test]
fn target_must_exist_and_prefer_idle_cards() {
    let mut roster = vec![
        entry("Alan", true, false),
        entry("Bob", true, true),
        entry("Cora", false, false),
        RosterLite {
            enabled: false,
            ..entry("Dan", true, false)
        },
    ];
    assert!(check_target(&roster, "Alan").is_ok());
    assert!(
        check_target(&roster, "Bob").is_ok(),
        "忙碌的也能提，派发时再看"
    );
    assert!(check_target(&roster, "Nobody").is_err());
    assert!(check_target(&roster, "Dan").is_err(), "停用的不能提");
    let error = check_target(&roster, "Cora").unwrap_err();
    assert!(error.contains("Alan"), "应点名空闲的 Alan：{error}");
    roster[0].busy = true;
    assert!(
        check_target(&roster, "Cora").is_ok(),
        "没有空闲卡片时可以新建"
    );
}
