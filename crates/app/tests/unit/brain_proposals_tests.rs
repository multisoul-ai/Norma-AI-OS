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

/// 数据迁移完成后懒加载另一个项目时，仍要拒绝损坏的重复提议编号。
/// 数据：有效alpha快照的同一编号出现两个不同完整任务。
/// 执行：通过生产scoped读取接口加载。
/// 预期：显式报错，不能让用户操作编号时消费错误的任务。
#[test]
fn scoped_proposals_reject_duplicate_ids_during_lazy_loading() {
    let mut proposals = Proposals::default();
    proposals
        .add(draft("Alan", "检查迁移"), SystemTime::UNIX_EPOCH)
        .unwrap();
    let mut saved: Value = serde_json::from_str(&proposals.to_scoped_json("alpha")).unwrap();
    let mut duplicate = saved["items"][0].clone();
    duplicate["task"] = json!("another full task");
    saved["items"].as_array_mut().unwrap().push(duplicate);
    assert!(
        Proposals::from_scoped_json(&saved.to_string(), "alpha").is_err(),
        "同一快照中的重复编号不能被加载为有效提议"
    );
}

/// 场景：超长的新建议不能先删掉同 Agent 已经等待用户处理的建议。
/// 构造：先存一条有效提议，再提交 51 字符标题；另构造旧版长文案快照。
/// 执行：使用生产 add/from_json，而非 UI 字符截取。
/// 预期：新文案被拒，旧提议与编号不变；历史长数据仍完整兼容读取。
#[test]
fn invalid_decision_copy_preserves_pending_proposals_and_legacy_data() {
    let mut proposals = Proposals::default();
    let id = proposals
        .add(draft("Henry", "补回归测试"), SystemTime::UNIX_EPOCH)
        .unwrap();
    let before = proposals.to_json();
    assert!(
        proposals
            .add(draft("Henry", &"字".repeat(51)), SystemTime::UNIX_EPOCH)
            .is_err(),
        "51 字符标题必须在替换前拒绝"
    );
    assert_eq!(
        proposals.to_json(),
        before,
        "校验失败不能删除旧建议或消耗编号"
    );
    assert_eq!(proposals.items()[0].id, id, "待处理提议的身份必须保持");
    let mut legacy: Value = serde_json::from_str(&before).unwrap();
    legacy["items"][0]["title"] = json!("字".repeat(80));
    let restored = Proposals::from_json(&legacy.to_string()).unwrap();
    assert_eq!(
        restored.items()[0].title.chars().count(),
        80,
        "历史长数据不能被删除或截断"
    );
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

/// 写入再读回，内容与编号都保留；损坏的文件返回错误。
///
/// 数据构造：两条提议（时间不同）序列化后再解析；另给一段非 JSON 文本。
/// 预期结果：
///   - 正断言：读回的提议与原来逐字段相同，下一个编号接着往后
///   - 负断言：损坏的文本报错，不会当成空列表覆盖
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
    let mut back = Proposals::from_json(&proposals.to_json()).expect("应解析有效快照");
    assert_eq!(back.items(), proposals.items(), "完整内容必须恢复");
    let next = back.add(draft("C", "三"), SystemTime::UNIX_EPOCH).unwrap();
    assert!(next > 2, "编号接着往后：{next}");
    assert!(
        Proposals::from_json("not json").is_err(),
        "损坏文件不能当成空列表"
    );
}

/// 场景：派发或忽略只取走被点中的那一条提议。
/// 数据：A 先加入得到编号 1，B 随后得到编号 2。
/// 执行：take(1)，再 take(1)，再 take(9)。
/// 预期：第一次得到 A；列表只剩 B；重复和不存在都是 None，B 的编号不变。
#[test]
fn take_removes_only_the_chosen_proposal() {
    let mut proposals = Proposals::default();
    let first = proposals
        .add(draft("A", "一"), SystemTime::UNIX_EPOCH)
        .expect("第一条应写入");
    let second = proposals
        .add(draft("B", "二"), SystemTime::UNIX_EPOCH)
        .expect("第二条应写入");
    let taken = proposals.take(first).expect("编号 1 必须被取走");
    assert_eq!(taken.agent, "A", "取走的是 A，不是 B");
    assert_eq!(taken.id, first, "取走的编号就是 1");
    assert_eq!(proposals.items().len(), 1, "列表只剩一条");
    assert_eq!(proposals.items()[0].id, second, "剩下的是 B");
    assert!(proposals.take(first).is_none(), "同一条不能再取一次");
    assert!(proposals.take(9).is_none(), "不存在的编号不能取走任何提议");
    assert_eq!(proposals.items()[0].agent, "B", "失败的取走不能删掉 B");
}

/// 合法 JSON 里任一提议坏掉时也必须整份拒绝，不能静默删除该条。
/// 数据构造：一份正常快照，分别删除 why、改错 at 类型、破坏顶层 items。
/// 执行过程：逐份解析损坏副本，并以正常快照作对照。
/// 预期结果：正常文件恢复一条；各损坏副本报错，调用者得以保留磁盘原件。
#[test]
fn structurally_invalid_json_is_never_loaded_as_an_empty_list() {
    let mut proposals = Proposals::default();
    proposals
        .add(draft("A", "完整提议"), SystemTime::UNIX_EPOCH)
        .unwrap();
    let value: Value = serde_json::from_str(&proposals.to_json()).unwrap();
    assert_eq!(
        Proposals::from_json(&value.to_string())
            .unwrap()
            .items()
            .len(),
        1,
        "正常快照应恢复"
    );
    let mut missing = value.clone();
    missing["items"][0].as_object_mut().unwrap().remove("why");
    let mut wrong = value;
    wrong["items"][0]["at"] = json!("oops");
    for invalid in [
        missing,
        wrong,
        json!({"next_id": 1}),
        json!({"next_id": 1, "items": null}),
    ] {
        assert!(
            Proposals::from_json(&invalid.to_string()).is_err(),
            "不能静默丢弃损坏条目：{invalid}"
        );
    }
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
