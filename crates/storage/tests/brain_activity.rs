use storage::{
    BrainActionRow, BrainActivityBatch, BrainActivityStore, BrainItemRow, BrainTurnUpsert,
    BRAIN_ACTIVITY_MAX_TEXT_BYTES, BRAIN_ACTIVITY_RETENTION_MS,
};
use tempfile::TempDir;

const DAY: i64 = 24 * 3600 * 1000;
const NOW: i64 = 100 * DAY;

fn store() -> (TempDir, BrainActivityStore) {
    let dir = TempDir::new().expect("临时目录");
    let store = BrainActivityStore::new(dir.path().join("app.sqlite"));
    (dir, store)
}

fn turn(launch: i64, area: &str, number: i64, started: i64) -> BrainTurnUpsert {
    BrainTurnUpsert {
        launch_id: launch,
        area_id: area.into(),
        lane: "Main".into(),
        turn: number,
        started_at_ms: started,
        trigger_kind: "agent_finished".into(),
        trigger_who: "Alan".into(),
        finished_at_ms: None,
        stop_reason: None,
    }
}

fn item(launch: i64, area: &str, number: i64, at: i64, text: &str) -> BrainItemRow {
    BrainItemRow {
        launch_id: launch,
        area_id: area.into(),
        lane: "Main".into(),
        turn: number,
        at_ms: at,
        kind: "reply".into(),
        title: String::new(),
        text: text.into(),
    }
}

fn action(launch: i64, at: i64, via: Option<i64>) -> BrainActionRow {
    BrainActionRow {
        launch_id: launch,
        area_id: "a".into(),
        at_ms: at,
        kind: "assign".into(),
        agent: "Chloe".into(),
        node_id: 3,
        summary: "跑测试".into(),
        reason: "只读".into(),
        ask_key: "assign".into(),
        via_lane: via.map(|_| "Main".into()),
        via_turn: via,
    }
}

/// 写入一批后能按页读回；轮次倒序，正文按写入顺序。
///
/// 数据构造：同一启动里 3 个轮次，第 2 轮有两条正文，另有一条动作。
/// 预期结果：
///   - 正断言：第一页 2 条是第 3、2 轮；游标翻页拿到第 1 轮；第 2 轮正文顺序与写入一致；动作字段完整
///   - 负断言：翻页不重复、不遗漏
#[test]
fn saved_batch_is_read_back_in_pages() {
    let (_dir, store) = store();
    store
        .save_batch(&BrainActivityBatch {
            turns: vec![
                turn(1, "a", 1, 1000),
                turn(1, "a", 2, 2000),
                turn(1, "a", 3, 3000),
            ],
            items: vec![item(1, "a", 2, 2001, "先"), item(1, "a", 2, 2002, "后")],
            actions: vec![action(1, 2500, Some(2))],
        })
        .expect("写入");
    let first = store.recent_turns(2, None).expect("第一页");
    assert_eq!(first.iter().map(|t| t.turn).collect::<Vec<_>>(), vec![3, 2]);
    let next = store
        .recent_turns(2, Some(first[1].started_at_ms))
        .expect("第二页");
    assert_eq!(next.iter().map(|t| t.turn).collect::<Vec<_>>(), vec![1]);
    let items = store.turn_items(1, "a", "Main", 2).expect("正文");
    assert_eq!(
        items.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(),
        vec!["先", "后"]
    );
    let actions = store.recent_actions(10, None).expect("动作");
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].via_turn, Some(2));
    assert_eq!(actions[0].summary, "跑测试");
    assert_eq!(first[0].trigger_who, "Alan");
}

/// 轮次结束信息后到：同一主键再次写入只补结束字段，不覆盖开始时间与触发源。
///
/// 数据构造：先写开始（无结束），再写同一轮的结束。
#[test]
fn turn_end_updates_the_existing_turn_without_losing_its_start() {
    let (_dir, store) = store();
    store
        .save_batch(&BrainActivityBatch {
            turns: vec![turn(1, "a", 1, 1000)],
            ..Default::default()
        })
        .expect("开始");
    let mut end = turn(1, "a", 1, 9999);
    end.trigger_kind = "unknown".into();
    end.trigger_who = String::new();
    end.finished_at_ms = Some(1500);
    end.stop_reason = Some("end_turn".into());
    store
        .save_batch(&BrainActivityBatch {
            turns: vec![end],
            ..Default::default()
        })
        .expect("结束");
    let turns = store.recent_turns(10, None).expect("读取");
    assert_eq!(turns.len(), 1, "同一主键只有一行");
    assert_eq!(turns[0].started_at_ms, 1000, "开始时间不被覆盖");
    assert_eq!(turns[0].trigger_kind, "agent_finished", "触发源不被覆盖");
    assert_eq!(turns[0].finished_at_ms, Some(1500));
    assert_eq!(turns[0].stop_reason.as_deref(), Some("end_turn"));
}

/// 轮次号每次启动从头计数：不同启动可同号并存，互不串内容。
#[test]
fn the_same_turn_number_in_different_launches_does_not_collide() {
    let (_dir, store) = store();
    store
        .save_batch(&BrainActivityBatch {
            turns: vec![turn(1, "a", 1, 1000), turn(2, "a", 1, 2000)],
            items: vec![item(1, "a", 1, 1001, "旧"), item(2, "a", 1, 2001, "新")],
            actions: vec![],
        })
        .expect("写入");
    assert_eq!(store.recent_turns(10, None).expect("读取").len(), 2);
    let old = store.turn_items(1, "a", "Main", 1).expect("旧");
    let new = store.turn_items(2, "a", "Main", 1).expect("新");
    assert_eq!((old[0].text.as_str(), new[0].text.as_str()), ("旧", "新"));
}

/// 清理只删 7 天前的内容，连同其正文；刚好在边界上的保留。
///
/// 数据构造：NOW 时刻清理；轮次分别在 8 天前、恰好 7 天前、1 天前；动作同理。
/// 预期结果：
///   - 正断言：8 天前的轮次、它的正文、8 天前的动作被删；7 天前整点与 1 天前保留
///   - 负断言：保留轮次的正文仍在
#[test]
fn prune_removes_only_data_older_than_seven_days() {
    let (_dir, store) = store();
    assert_eq!(BRAIN_ACTIVITY_RETENTION_MS, 7 * DAY);
    store
        .save_batch(&BrainActivityBatch {
            turns: vec![
                turn(1, "a", 1, NOW - 8 * DAY),
                turn(1, "a", 2, NOW - 7 * DAY),
                turn(1, "a", 3, NOW - DAY),
            ],
            items: vec![
                item(1, "a", 1, NOW - 8 * DAY + 1, "过期"),
                item(1, "a", 2, NOW - 7 * DAY + 1, "边界"),
                item(1, "a", 3, NOW - DAY + 1, "新"),
            ],
            actions: vec![action(1, NOW - 8 * DAY, None), action(1, NOW - DAY, None)],
        })
        .expect("写入");
    let report = store.prune(NOW).expect("清理");
    assert_eq!((report.turns, report.actions), (1, 1));
    let turns = store.recent_turns(10, None).expect("读取");
    assert_eq!(turns.iter().map(|t| t.turn).collect::<Vec<_>>(), vec![3, 2]);
    assert!(store
        .turn_items(1, "a", "Main", 1)
        .expect("过期")
        .is_empty());
    assert_eq!(store.turn_items(1, "a", "Main", 2).expect("边界").len(), 1);
    assert_eq!(store.recent_actions(10, None).expect("动作").len(), 1);
}

/// 体积兜底：正文总量超过上限时从最旧的整轮开始删，直到回到上限内。
///
/// 数据构造：每轮一条接近上限三分之一的正文，共 4 轮，时间均在 7 天内。
/// 预期结果：最旧的轮次被删，最新的保留；总量不超过上限。
#[test]
fn prune_enforces_the_text_byte_cap_oldest_turns_first() {
    let (_dir, store) = store();
    let big = "x".repeat(BRAIN_ACTIVITY_MAX_TEXT_BYTES / 3);
    let mut batch = BrainActivityBatch::default();
    for n in 1..=4 {
        batch.turns.push(turn(1, "a", n, NOW - 4 * DAY + n * 1000));
        batch
            .items
            .push(item(1, "a", n, NOW - 4 * DAY + n * 1000, &big));
    }
    store.save_batch(&batch).expect("写入");
    let report = store.prune(NOW).expect("清理");
    assert!(report.turns >= 1, "至少删掉一整轮");
    let turns = store.recent_turns(10, None).expect("读取");
    assert_eq!(turns[0].turn, 4, "最新的保留");
    assert!(!turns.iter().any(|t| t.turn == 1), "最旧的先删");
    assert!(store.text_bytes().expect("体积") <= BRAIN_ACTIVITY_MAX_TEXT_BYTES);
}

/// 清空：设置页“清除幕僚活动”用，三张表都空。
#[test]
fn clear_empties_everything() {
    let (_dir, store) = store();
    store
        .save_batch(&BrainActivityBatch {
            turns: vec![turn(1, "a", 1, 1000)],
            items: vec![item(1, "a", 1, 1001, "x")],
            actions: vec![action(1, 1002, None)],
        })
        .expect("写入");
    store.clear().expect("清空");
    assert!(store.recent_turns(10, None).expect("轮次").is_empty());
    assert!(store.recent_actions(10, None).expect("动作").is_empty());
    assert_eq!(store.text_bytes().expect("体积"), 0);
}

/// 空批次不打开数据库写事务：路径所在目录不存在时也不报错、不创建文件。
#[test]
fn an_empty_batch_is_a_noop() {
    let dir = TempDir::new().expect("临时目录");
    let path = dir.path().join("missing").join("app.sqlite");
    BrainActivityStore::new(path.clone())
        .save_batch(&BrainActivityBatch::default())
        .expect("空批次");
    assert!(!path.exists());
}

fn typed_item(number: i64, at: i64, kind: &str, text: &str) -> BrainItemRow {
    BrainItemRow {
        kind: kind.into(),
        ..item(1, "a", number, at, text)
    }
}

/// 折叠行需要的预览：各种类条数按首次出现的顺序，摘要取最后一条非空回复，没有回复就取提示。
///
/// 数据构造：第 1 轮 prompt、tool_call、tool_result×2、reply×2（后一条为空串之前的“结论”）；
/// 第 2 轮只有 prompt；第 3 轮没有任何正文。
/// 预期结果：
///   - 正断言：第 1 轮条数依次 prompt1/tool_call1/tool_result2/reply2，摘要是最后一条非空回复；
///     第 2 轮摘要取 prompt；第 3 轮条数为空、摘要为空
///   - 负断言：摘要最多 300 个字符
#[test]
fn turn_previews_summarise_kinds_and_pick_the_last_reply() {
    let (_dir, store) = store();
    let long = "长".repeat(500);
    store
        .save_batch(&BrainActivityBatch {
            turns: vec![
                turn(1, "a", 1, 1000),
                turn(1, "a", 2, 2000),
                turn(1, "a", 3, 3000),
            ],
            items: vec![
                typed_item(1, 1001, "prompt", "问"),
                typed_item(1, 1002, "tool_call", "t"),
                typed_item(1, 1003, "tool_result", "r1"),
                typed_item(1, 1004, "tool_result", "r2"),
                typed_item(1, 1005, "reply", "先说"),
                typed_item(1, 1006, "reply", &long),
                typed_item(1, 1007, "reply", ""),
                typed_item(2, 2001, "prompt", "只有提示"),
            ],
            actions: vec![],
        })
        .expect("写入");
    let turns = store.recent_turns(10, None).expect("轮次");
    let previews = store.turn_previews(&turns).expect("预览");
    let find = |n: i64| previews.iter().find(|p| p.turn == n).expect("有预览");
    assert_eq!(
        find(1).counts,
        vec![
            ("prompt".to_string(), 1),
            ("tool_call".to_string(), 1),
            ("tool_result".to_string(), 2),
            ("reply".to_string(), 3),
        ]
    );
    assert_eq!(find(1).summary.chars().count(), 300);
    assert_eq!(find(2).summary, "只有提示");
    assert!(find(3).counts.is_empty() && find(3).summary.is_empty());
}

/// 未读动作数：只数“上次打开之后”的动作；标记已读后归零，之后的新动作再计数。
#[test]
fn unseen_actions_count_only_those_after_the_last_mark() {
    let (_dir, store) = store();
    store
        .save_batch(&BrainActivityBatch {
            actions: vec![action(1, 1000, None), action(1, 2000, None)],
            ..Default::default()
        })
        .expect("写入");
    assert_eq!(store.unseen_actions().expect("未读"), 2);
    store.mark_actions_seen(1500).expect("标记");
    assert_eq!(store.unseen_actions().expect("未读"), 1, "只剩 2000 那条");
    store.mark_actions_seen(5000).expect("标记");
    assert_eq!(store.unseen_actions().expect("未读"), 0);
    store
        .save_batch(&BrainActivityBatch {
            actions: vec![action(1, 6000, None)],
            ..Default::default()
        })
        .expect("写入");
    assert_eq!(store.unseen_actions().expect("未读"), 1);
}

/// 清空记录不重置“已读”位置，否则清空后第一条新动作之前的旧时间戳会被误当成未读。
#[test]
fn clear_keeps_the_seen_marker() {
    let (_dir, store) = store();
    store.mark_actions_seen(9000).expect("标记");
    store.clear().expect("清空");
    store
        .save_batch(&BrainActivityBatch {
            actions: vec![action(1, 8000, None)],
            ..Default::default()
        })
        .expect("写入");
    assert_eq!(store.unseen_actions().expect("未读"), 0);
}
