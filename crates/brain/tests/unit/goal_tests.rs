use super::*;

fn store() -> (tempfile::TempDir, GoalStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = GoalStore::open(dir.path().to_path_buf());
    (dir, store)
}

/// 只保留最近 5 条目标，旧的丢弃，顺序是旧到新。
///
/// 数据构造：依次记录 7 条不同的任务。
/// 预期结果：
///   - 正断言：剩 5 条，第一条是第 3 条任务，最后一条是第 7 条
///   - 负断言：第 1、2 条已被丢弃
#[test]
fn keeps_only_the_latest_five_goals() {
    let (_dir, store) = store();
    for n in 1..=7 {
        assert!(store.record("Alan", &format!("实现第{n}个功能并补测试")));
    }
    let goals = store.goals();
    assert_eq!(goals.len(), 5);
    assert!(goals[0].text.contains("第3个"), "{goals:?}");
    assert!(goals[4].text.contains("第7个"), "{goals:?}");
}

/// 太短的话和斜杠命令不当作目标；同一 Agent 连续重复的内容只记一次。
///
/// 数据构造：“好的”“继续”“/new claude”各一次；同一句长任务连续两次。
/// 预期结果：
///   - 正断言：长任务只记一次，返回值 true 然后 false
///   - 负断言：三条短内容都返回 false，目标列表里没有它们
#[test]
fn trivial_and_repeated_inputs_are_not_goals() {
    let (_dir, store) = store();
    for noise in ["好的", "继续", "/new claude"] {
        assert!(!store.record("Alan", noise), "{noise} 不应被记录");
    }
    assert!(store.record("Alan", "把登录页的校验补完整"));
    assert!(
        !store.record("Alan", "把登录页的校验补完整"),
        "连续重复不再记录"
    );
    assert_eq!(store.goals().len(), 1);
}

/// 目标落盘，重新打开后内容与顺序一致。
#[test]
fn goals_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    {
        let store = GoalStore::open(dir.path().to_path_buf());
        store.record("Alan", "实现 slugify，之后让别的 Agent 补测试");
        store.record("Bella", "整理文档目录结构并更新索引");
    }
    let reopened = GoalStore::open(dir.path().to_path_buf());
    let goals = reopened.goals();
    assert_eq!(goals.len(), 2);
    assert_eq!(goals[0].agent, "Alan");
    assert_eq!(goals[1].text, "整理文档目录结构并更新索引");
}

/// 摘要每条一行带 Agent 名，换行被压平，过长的被截断；没有目标时是 None。
#[test]
fn summary_lists_one_line_per_goal() {
    let (_dir, store) = store();
    assert!(store.summary().is_none());
    store.record("Alan", "第一行\n第二行，一直写下去");
    let text = store.summary().expect("有目标");
    assert!(text.contains("Alan：第一行 第二行"), "{text}");
    assert_eq!(text.lines().count(), 1);
    store.record("Bella", &"长".repeat(500));
    assert!(store.summary().unwrap().chars().count() < 450);
}
