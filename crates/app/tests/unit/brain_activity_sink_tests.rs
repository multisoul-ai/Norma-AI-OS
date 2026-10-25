use super::super::log::AutoKind;
use super::*;
use brain::acp::trace::{AcpTrace, TraceItem, TraceKind};
use brain::{Trigger, TriggerKind};
use std::time::{Duration, SystemTime};
use storage::BrainActivityStore;

fn at(ms: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_millis(ms)
}

fn trace(turn: u64, ms: u64, item: TraceItem) -> Pending {
    Pending::Trace(AcpTrace {
        at: at(ms),
        lane: "Main".into(),
        turn,
        trigger: Trigger::new(TriggerKind::AgentFinished, "Alan"),
        item,
    })
}

fn action(ms: u64, kind: AutoKind) -> AutoAction {
    AutoAction {
        at: at(ms),
        kind,
        agent: "Chloe".into(),
        node_id: 3,
        summary: "跑测试".into(),
        reason: "只读".into(),
        ask_key: "assign".into(),
    }
}

fn store(dir: &tempfile::TempDir) -> BrainActivityStore {
    BrainActivityStore::new(dir.path().join("app.sqlite"))
}

/// 一批记录转成存储行：同一轮只留一行轮次，开始取最早一条，结束取 Turn 条目。
///
/// 数据构造：第 4 轮的 Prompt（1000ms）、Reply（1500ms）、Turn 结束（2000ms，end_turn），
/// 加一条主会话动作（来自第 4 轮）和一条用户批准的派发。
/// 预期结果：
///   - 正断言：1 行轮次（开始 1000、结束 2000、停止原因 end_turn、触发 agent_finished/Alan）；
///     3 行正文；2 行动作，第一条 via=Main/4，第二条无 via，种类分别是 assigned 与 user
///   - 负断言：正文行不带结束信息
#[test]
fn a_batch_of_entries_becomes_one_turn_with_its_items_and_actions() {
    let pending = vec![
        trace(4, 1000, TraceItem::new(TraceKind::Prompt, "发给大脑", "p")),
        trace(4, 1500, TraceItem::new(TraceKind::Reply, "", "r")),
        trace(4, 2000, TraceItem::turn_end("end_turn", 1000)),
        Pending::Action {
            action: action(1800, AutoKind::Assigned),
            via: Some(("Main".into(), 4)),
        },
        Pending::Action {
            action: action(1900, AutoKind::UserDispatched),
            via: None,
        },
    ];
    let batch = build_batch(7, "area-1", pending);
    assert_eq!(batch.turns.len(), 1, "{:?}", batch.turns);
    let turn = &batch.turns[0];
    assert_eq!(
        (turn.launch_id, turn.turn, turn.started_at_ms),
        (7, 4, 1000)
    );
    assert_eq!(turn.finished_at_ms, Some(2000));
    assert_eq!(turn.stop_reason.as_deref(), Some("end_turn"));
    assert_eq!(
        (turn.trigger_kind.as_str(), turn.trigger_who.as_str()),
        ("agent_finished", "Alan")
    );
    assert_eq!(batch.items.len(), 3);
    assert_eq!(batch.items[0].kind, "prompt");
    assert!(batch.items.iter().all(|i| i.area_id == "area-1"));
    assert_eq!(batch.actions.len(), 2);
    assert_eq!(
        (batch.actions[0].kind.as_str(), batch.actions[0].via_turn),
        ("assigned", Some(4))
    );
    assert_eq!(batch.actions[0].via_lane.as_deref(), Some("Main"));
    assert_eq!(
        (batch.actions[1].kind.as_str(), batch.actions[1].via_turn),
        ("user", None)
    );
}

/// 写入器在后台落库，关闭时把队列里的批次写完。
///
/// 数据构造：启动写入器，提交一批后立即 shutdown。
/// 预期结果：shutdown 返回 true（限时内完成），库里能读到那一轮与动作。
#[test]
fn writer_persists_submitted_batches_before_shutdown_returns() {
    let dir = tempfile::tempdir().expect("临时目录");
    let writer = ActivityWriter::start(dir.path().join("app.sqlite"));
    let batch = build_batch(
        writer.launch_id(),
        "a",
        vec![
            trace(1, 1000, TraceItem::new(TraceKind::Prompt, "", "p")),
            Pending::Action {
                action: action(1100, AutoKind::Approved),
                via: None,
            },
        ],
    );
    assert!(writer.submit(batch).is_none(), "队列空闲，不应有剩余");
    assert!(writer.shutdown(Duration::from_secs(5)), "应在限时内写完");
    let store = store(&dir);
    assert_eq!(store.recent_turns(10, None).expect("轮次").len(), 1);
    assert_eq!(store.recent_actions(10, None).expect("动作").len(), 1);
}

/// 队列满时丢轮次正文并计数，但动作原样退还给调用方下次重试。
///
/// 数据构造：容量 1 且没有消费线程的写入器，连续提交 3 批，每批一条正文、一条动作。
/// 预期结果：
///   - 正断言：第 1 批入队；第 2、3 批各退还 1 条动作；丢弃计数 = 2
///   - 负断言：退还的批次里没有轮次和正文
#[test]
fn a_full_queue_drops_trace_text_but_hands_actions_back() {
    let writer = ActivityWriter::detached(1);
    let make = |n: u64| {
        build_batch(
            writer.launch_id(),
            "a",
            vec![
                trace(n, 1000 + n, TraceItem::new(TraceKind::Reply, "", "x")),
                Pending::Action {
                    action: action(2000 + n, AutoKind::Denied),
                    via: None,
                },
            ],
        )
    };
    assert!(writer.submit(make(1)).is_none());
    for n in [2, 3] {
        let back = writer.submit(make(n)).expect("队列已满，应退还");
        assert_eq!(back.actions.len(), 1);
        assert!(back.turns.is_empty() && back.items.is_empty());
    }
    assert_eq!(writer.dropped(), 2);
}

/// 写入器启动时清理 7 天前的记录。
///
/// 数据构造：库里预先有一条 10 天前的轮次与一条刚才的轮次。
/// 预期结果：启动并关闭后只剩新的一条。
#[test]
fn writer_prunes_expired_rows_on_start() {
    let dir = tempfile::tempdir().expect("临时目录");
    let now = unix_ms(SystemTime::now());
    let turn = |n: i64, started: i64| storage::BrainTurnUpsert {
        launch_id: 1,
        area_id: "a".into(),
        lane: "Main".into(),
        turn: n,
        started_at_ms: started,
        trigger_kind: "unknown".into(),
        trigger_who: String::new(),
        finished_at_ms: None,
        stop_reason: None,
    };
    store(&dir)
        .save_batch(&storage::BrainActivityBatch {
            turns: vec![turn(1, now - 10 * 24 * 3600 * 1000), turn(2, now)],
            ..Default::default()
        })
        .expect("预置");
    let writer = ActivityWriter::start(dir.path().join("app.sqlite"));
    assert!(writer.shutdown(Duration::from_secs(5)));
    let turns = store(&dir).recent_turns(10, None).expect("读取");
    assert_eq!(turns.iter().map(|t| t.turn).collect::<Vec<_>>(), vec![2]);
}

/// 动作标注“来自第几轮”：放行与拒绝取审批会话最近一轮，派发取主会话最近一轮，用户批准的没有。
///
/// 数据构造：主会话走到第 3 轮、审批会话走到第 5 轮后，依次记一条派发、放行、用户派发；
/// 另一个全新的状态里直接记一条放行（审批会话还没有任何一轮）。
/// 预期结果：
///   - 正断言：三条动作的来源依次为 Main/3、Approval/5、无；发件箱里还有 2 条轮次记录
///   - 负断言：没有轮次可对应时来源为空，而不是 0
#[test]
fn recorded_actions_point_at_the_latest_turn_of_their_lane() {
    let mut state = super::super::BrainHostState::new();
    let mut put = |lane: &str, turn: u64| {
        state.record_trace(AcpTrace {
            at: at(1000),
            lane: lane.into(),
            turn,
            trigger: Trigger::unknown(),
            item: TraceItem::new(TraceKind::Prompt, "", "p"),
        })
    };
    put("Main", 3);
    put("Approval", 5);
    state.record_action(action(2000, AutoKind::Assigned));
    state.record_action(action(2001, AutoKind::Approved));
    state.record_action(action(2002, AutoKind::UserDispatched));
    let pending = state.activity_outbox.take();
    let vias: Vec<Option<(String, u64)>> = pending
        .iter()
        .filter_map(|p| match p {
            Pending::Action { via, .. } => Some(via.clone()),
            Pending::Trace(_) => None,
        })
        .collect();
    assert_eq!(
        vias,
        vec![Some(("Main".into(), 3)), Some(("Approval".into(), 5)), None]
    );
    assert_eq!(pending.len() - vias.len(), 2, "两条轮次记录也在发件箱里");

    let mut fresh = super::super::BrainHostState::new();
    fresh.record_action(action(3000, AutoKind::Approved));
    match &fresh.activity_outbox.take()[0] {
        Pending::Action { via, .. } => assert_eq!(*via, None),
        Pending::Trace(_) => panic!("应当是动作"),
    }
}

/// 发件箱封顶：没人取走时丢最旧的，留下最新的。
#[test]
fn the_outbox_keeps_only_the_newest_entries() {
    let outbox = Outbox::default();
    for n in 0..(OUTBOX_CAP as u64 + 10) {
        outbox.push(trace(n, n, TraceItem::new(TraceKind::Reply, "", "x")));
    }
    let kept = outbox.take();
    assert_eq!(kept.len(), OUTBOX_CAP);
    match (&kept[0], kept.last().expect("非空")) {
        (Pending::Trace(first), Pending::Trace(last)) => {
            assert_eq!((first.turn, last.turn), (10, OUTBOX_CAP as u64 + 9));
        }
        _ => panic!("应当都是轮次记录"),
    }
    assert!(outbox.take().is_empty(), "取走后清空");
}
