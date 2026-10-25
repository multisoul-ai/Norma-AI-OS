use super::super::activity_feed::FeedTrigger;
use super::*;
use brain::TriggerKind;
use storage::{
    BrainActionRow, BrainActivityBatch, BrainActivityStore, BrainItemRow, BrainTurnUpsert,
};

fn turn(n: i64, lane: &str, at: i64, kind: &str) -> BrainTurnUpsert {
    BrainTurnUpsert {
        launch_id: 1,
        area_id: "a".into(),
        lane: lane.into(),
        turn: n,
        started_at_ms: at,
        trigger_kind: kind.into(),
        trigger_who: "Alan".into(),
        finished_at_ms: Some(at + 10),
        stop_reason: Some("end_turn".into()),
    }
}

fn item(n: i64, at: i64, kind: &str, text: &str) -> BrainItemRow {
    BrainItemRow {
        launch_id: 1,
        area_id: "a".into(),
        lane: "Main".into(),
        turn: n,
        at_ms: at,
        kind: kind.into(),
        title: String::new(),
        text: text.into(),
    }
}

fn action(at: i64, via: Option<i64>) -> BrainActionRow {
    BrainActionRow {
        launch_id: 1,
        area_id: "a".into(),
        at_ms: at,
        kind: "assigned".into(),
        agent: "Chloe".into(),
        node_id: 3,
        summary: "跑测试".into(),
        reason: "只读".into(),
        ask_key: "assign".into(),
        via_lane: via.map(|_| "Main".into()),
        via_turn: via,
    }
}

fn view_with_rows() -> ActivityView {
    let mut view = ActivityView::default();
    view.apply(ActivitySnapshot {
        rows: build_feed(
            vec![
                turn(1, "Main", 1000, "agent_finished"),
                turn(2, "Approval", 2000, "approval_request"),
            ],
            vec![],
            vec![action(1100, Some(1))],
        ),
        details: Default::default(),
        unseen: 3,
        ..Default::default()
    });
    view
}

fn source_key() -> TurnKey {
    (1, "a".into(), "Main".into(), 1)
}

/// 从库里读一页快照：轮次、预览、动作、展开轮次的正文、未读数一次取齐。
///
/// 数据构造：第 1 轮有提示与回复，第 2 轮没有正文；一个来自第 1 轮的动作；
/// 只展开第 1 轮；标记已读到 1050 之前，所以那个动作（1100）算未读。
/// 预期结果：
///   - 正断言：3 行（动作在第 1 轮之后）；第 1 轮摘要是回复文本、正文 2 条且种类还原；未读 1
///   - 负断言：没展开的第 2 轮没有正文；第 1 轮正文里没有结束信息
#[test]
fn snapshot_reads_rows_previews_details_and_unseen() {
    let dir = tempfile::tempdir().expect("临时目录");
    let path = dir.path().join("app.sqlite");
    let store = BrainActivityStore::new(path.clone());
    store
        .save_batch(&BrainActivityBatch {
            turns: vec![
                turn(1, "Main", 1000, "agent_finished"),
                turn(2, "Approval", 2000, "approval_request"),
            ],
            items: vec![item(1, 1001, "prompt", "问"), item(1, 1002, "reply", "答")],
            actions: vec![action(1100, Some(1))],
        })
        .expect("写入");
    store.mark_actions_seen(1050).expect("标记");
    let snapshot = read_snapshot(path, 1, vec![source_key()]).expect("读取");
    assert_eq!(snapshot.rows.len(), 3);
    assert_eq!(snapshot.unseen, 1);
    let FeedRow::Turn(first) = snapshot.rows.last().expect("有行") else {
        panic!("第 1 轮")
    };
    assert_eq!((first.key.3, first.summary.as_str()), (1, "答"));
    let items = &snapshot.details[&source_key()];
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].kind, brain::acp::trace::TraceKind::Prompt);
    assert_eq!(items[1].kind, brain::acp::trace::TraceKind::Reply);
    assert!(items.iter().all(|i| i.end.is_none()));
    assert_eq!(snapshot.details.len(), 1, "只读展开的那一轮");
}

/// 分页：页数越多读得越多，读不满一页说明到底了。
#[test]
fn more_pages_load_more_rows() {
    let dir = tempfile::tempdir().expect("临时目录");
    let path = dir.path().join("app.sqlite");
    let turns: Vec<BrainTurnUpsert> = (1..=(PAGE_SIZE as i64 + 5))
        .map(|n| turn(n, "Main", n * 1000, "agent_finished"))
        .collect();
    BrainActivityStore::new(path.clone())
        .save_batch(&BrainActivityBatch {
            turns,
            ..Default::default()
        })
        .expect("写入");
    let first = read_snapshot(path.clone(), 1, vec![]).expect("第一页");
    assert_eq!(first.rows.len(), PAGE_SIZE);
    assert!(first.may_have_more);
    let second = read_snapshot(path, 2, vec![]).expect("两页");
    assert_eq!(second.rows.len(), PAGE_SIZE + 5);
    assert!(!second.may_have_more);
}

/// 点“来自第 N 轮”：清掉筛选，展开那一轮并滚到它、短暂高亮；找不到时什么都不改。
///
/// 数据构造：先按“对话轮次”筛选（动作行会被藏起来），再跳到第 1 轮。
/// 预期结果：
///   - 正断言：返回 true；筛选恢复默认；第 1 轮已展开；待滚动与高亮都指向它
///   - 负断言：跳到不存在的轮次返回 false，筛选与展开状态不变
#[test]
fn jumping_to_the_source_turn_resets_filters_and_expands_it() {
    let mut view = view_with_rows();
    view.set_kind(FeedKind::Turns);
    assert!(view.jump_to(&source_key()));
    assert_eq!(view.filter, FeedFilter::default());
    assert!(view.is_expanded(&RowKey::Turn(source_key())));
    assert_eq!(view.scroll_to, Some(RowKey::Turn(source_key())));
    assert!(matches!(&view.flash, Some((RowKey::Turn(k), _)) if *k == source_key()));
    let mut other = view_with_rows();
    other.set_kind(FeedKind::Actions);
    assert!(!other.jump_to(&(1, "a".into(), "Main".into(), 99)));
    assert_eq!(other.filter.kind, FeedKind::Actions);
    assert!(other.expanded.is_empty());
}

/// 项目与触发源筛选：依次循环“全部 → 各项 → 全部”。
#[test]
fn area_and_trigger_filters_cycle_through_the_available_values() {
    let mut view = view_with_rows();
    assert_eq!(view.filter.trigger, None);
    view.cycle_trigger();
    assert_eq!(
        view.filter.trigger,
        Some(FeedTrigger::Brain(TriggerKind::ApprovalRequest))
    );
    view.cycle_trigger();
    assert_eq!(
        view.filter.trigger,
        Some(FeedTrigger::Brain(TriggerKind::AgentFinished))
    );
    view.cycle_trigger();
    assert_eq!(view.filter.trigger, None, "走完一圈回到全部");
    assert_eq!(view.filter.area, None);
    view.cycle_area();
    assert_eq!(view.filter.area.as_deref(), Some("a"));
    view.cycle_area();
    assert_eq!(view.filter.area, None);
}

/// 展开收起按行记录；刷新后仍在列表里的展开状态保留，已不在列表里的被丢掉。
#[test]
fn expansion_survives_refresh_only_for_rows_that_still_exist() {
    let mut view = view_with_rows();
    view.toggle(RowKey::Turn(source_key()));
    view.toggle(RowKey::Turn((1, "a".into(), "Main".into(), 77)));
    assert!(view.is_expanded(&RowKey::Turn(source_key())));
    view.apply(ActivitySnapshot {
        rows: view.rows.clone(),
        details: Default::default(),
        unseen: 0,
        ..Default::default()
    });
    assert!(view.is_expanded(&RowKey::Turn(source_key())));
    assert!(
        !view.is_expanded(&RowKey::Turn((1, "a".into(), "Main".into(), 77))),
        "已被清理的行不再占着状态"
    );
    view.toggle(RowKey::Turn(source_key()));
    assert!(
        !view.is_expanded(&RowKey::Turn(source_key())),
        "再点一次收起"
    );
}

/// 高亮 1 秒后自动消失，不留残影。
#[test]
fn the_flash_expires() {
    let mut view = view_with_rows();
    view.jump_to(&source_key());
    let at = view.flash.as_ref().expect("高亮").1;
    assert!(view.flashing(&RowKey::Turn(source_key()), at));
    let later = at + std::time::Duration::from_millis(1200);
    assert!(!view.flashing(&RowKey::Turn(source_key()), later));
    view.expire_flash(later);
    assert!(view.flash.is_none());
}

/// 清除要点两次：第一次只“上膛”，第二次才真正清除，之后恢复原状，避免手滑。
#[test]
fn clearing_needs_a_second_click() {
    let mut view = ActivityView::default();
    assert!(!view.request_clear(), "第一次只是确认");
    assert!(view.confirm_clear);
    assert!(view.request_clear(), "第二次才执行");
    assert!(!view.confirm_clear, "执行后恢复");
}

/// 跳转后的滚动要等展开那一轮的正文读到再执行：正文一到行高就变，提前滚会停在行的中部。
///
/// 数据构造：跳到第 1 轮（正文还没读到），随后快照带着该轮正文到达。
/// 预期结果：正文到达前没有可执行的滚动目标；到达后目标是第 1 轮；动作行没有正文依赖，立即可滚。
#[test]
fn the_jump_scroll_waits_for_the_turn_body() {
    let mut view = view_with_rows();
    assert!(view.jump_to(&source_key()));
    assert_eq!(view.scroll_target(), None, "正文还没到，不能滚");
    let mut details = std::collections::HashMap::new();
    details.insert(
        source_key(),
        vec![brain::acp::trace::TraceItem::new(
            brain::acp::trace::TraceKind::Prompt,
            "",
            "p",
        )],
    );
    view.apply(ActivitySnapshot {
        rows: view.rows.clone(),
        details,
        ..Default::default()
    });
    assert_eq!(view.scroll_target(), Some(RowKey::Turn(source_key())));
    view.scroll_to = Some(row_key(&view.rows[0]));
    let first_is_action = matches!(&view.rows[0], FeedRow::Action(_));
    assert_eq!(
        view.scroll_target().is_some(),
        first_is_action,
        "动作行不依赖正文"
    );
}
