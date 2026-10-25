use super::super::{activity_sink, BrainHost, BrainHostState};
use brain::acp::trace::{AcpTrace, TraceItem, TraceKind};
use brain::{BrainStatus, Trigger};
use std::sync::mpsc;
use std::time::SystemTime;

/// 场景：设置重启后额度错误迟到，旧轮必须结束，不能影响新 host 或混进新轮。
/// 数据构造：旧/新 host 各有独立状态通道，旧轮 43 的 Error 在 retire 后送达。
/// 执行过程：生产 retire/drain 路径分批落入真实 SQLite，再重新打开存储读取。
/// 预期结果：43 保存额度正文与 error 结束；44 仍成功；旧 BackendDown 不改新轮来源。
#[test]
fn retired_quota_error_survives_storage_reopen_without_crossing_turns() {
    let mut state = BrainHostState::new();
    let (tx, status) = mpsc::channel();
    state.host = Some(BrainHost {
        service: None,
        status,
        token: "old".into(),
    });
    let trace = |turn, item| AcpTrace {
        at: SystemTime::now(),
        lane: "Main".into(),
        turn,
        trigger: Trigger::unknown(),
        item,
    };
    state.record_trace(trace(
        43,
        TraceItem::new(
            TraceKind::Lifecycle,
            "会话关联",
            "backend=ClaudeCode session_id=old-quota",
        ),
    ));
    state.record_trace(trace(
        43,
        TraceItem::new(TraceKind::Prompt, "发给大脑", "额度错误回放（非历史轮次）"),
    ));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.sqlite");
    let store = storage::BrainActivityStore::new(path.clone());
    store
        .save_batch(&activity_sink::build_batch(
            123,
            "area",
            state.activity_outbox.take(),
        ))
        .unwrap();
    state.retire_host();
    let (_new_tx, status) = mpsc::channel();
    state.host = Some(BrainHost {
        service: None,
        status,
        token: "new".into(),
    });
    state.record_trace(trace(
        44,
        TraceItem::new(TraceKind::Prompt, "发给大脑", "新轮输入"),
    ));
    let body = "You've hit your session limit · resets 6:30am (America/Los_Angeles)";
    for item in [
        TraceItem::new(TraceKind::Reply, "", body),
        TraceItem::turn_end("error", 2500),
        TraceItem::new(TraceKind::Error, "本轮失败", format!("rate_limit: {body}")),
    ] {
        tx.send(BrainStatus::Trace(trace(43, item)))
            .expect("退休通道仍接收已发出的事件");
    }
    tx.send(BrainStatus::BackendDown {
        reason: "old quota".into(),
    })
    .unwrap();
    drop(tx);
    state.drain_retired();
    assert_eq!(state.last_turn["Main"], 44, "旧错误不能覆盖新轮来源");
    assert!(state.token_matches("new"), "新 host 保持有效");
    assert!(!state.token_matches("old"), "旧令牌无效");
    state.record_trace(trace(44, TraceItem::turn_end("end_turn", 300)));
    store
        .save_batch(&activity_sink::build_batch(
            123,
            "area",
            state.activity_outbox.take(),
        ))
        .unwrap();
    drop(store);
    let reopened = storage::BrainActivityStore::new(path);
    let turns = reopened.recent_turns(10, None).unwrap();
    assert_eq!(turns.len(), 2, "不能合并或新增幽灵轮次");
    for (turn, stop) in [(43, "error"), (44, "end_turn")] {
        let row = turns.iter().find(|t| t.turn == turn).expect("原轮次存在");
        assert_eq!(row.stop_reason.as_deref(), Some(stop), "停止原因不串轮");
        assert!(row.finished_at_ms.is_some(), "轮次必须结束");
        let items = reopened.turn_items(123, "area", "Main", turn).unwrap();
        assert_eq!(
            items.iter().filter(|i| i.kind == "turn").count(),
            1,
            "唯一结束记录"
        );
        assert_eq!(
            items
                .iter()
                .any(|i| i.kind == "error" && i.text.contains(body)),
            turn == 43,
            "额度错误只属于原轮"
        );
    }
    assert!(state.retiring.is_empty(), "通道排空后释放");
}

/// 场景：切换后端后，旧会话的权限拒绝和结束迟到，仍须持久化且不授权旧令牌。
/// 数据构造：旧 host 的公开状态通道，重启前记录输入，重启后发拒绝和取消。
/// 执行过程：走生产 retire/drain 路径，再把 outbox 交给真实活动存储并读取。
/// 预期结果：同轮输入、拒绝、结束均保留，旧令牌失效，迟到记录不覆盖新轮来源。
#[test]
fn retired_host_keeps_late_permission_and_end_in_activity() {
    let mut state = BrainHostState::new();
    let (tx, status) = mpsc::channel();
    state.host = Some(BrainHost {
        service: None,
        status,
        token: "old".into(),
    });
    let trace = |item| AcpTrace {
        at: SystemTime::now(),
        lane: "Main".into(),
        turn: 17,
        trigger: Trigger::unknown(),
        item,
    };
    state.record_trace(trace(TraceItem::new(
        TraceKind::Prompt,
        "发给大脑",
        "完整目标输入",
    )));
    state.retire_host();
    assert!(!state.token_matches("old"), "旧令牌必须立刻失效");
    state.last_turn.insert("Main".into(), 18);
    tx.send(BrainStatus::Trace(trace(TraceItem::new(
        TraceKind::Permission,
        "权限请求：拒绝",
        "非允许程序",
    ))))
    .expect("旧接收通道要活到后台停止完成");
    tx.send(BrainStatus::Trace(trace(TraceItem::turn_end(
        "cancelled",
        2500,
    ))))
    .unwrap();
    drop(tx);
    state.drain_retired();
    assert_eq!(state.last_turn["Main"], 18, "旧轮次不能覆盖当前动作来源");
    let batch = activity_sink::build_batch(123, "area", state.activity_outbox.take());
    let dir = tempfile::tempdir().unwrap();
    let store = storage::BrainActivityStore::new(dir.path().join("app.sqlite"));
    store.save_batch(&batch).expect("真实存储保存");
    let turns = store.recent_turns(10, None).unwrap();
    let stored_items = store
        .turn_items(123, "area", "Main", 17)
        .expect("从真实存储读取正文");
    assert_eq!(turns.len(), 1, "迟到事件必须归到原轮次");
    assert_eq!(
        turns[0].stop_reason.as_deref(),
        Some("cancelled"),
        "取消在持久化记录可见"
    );
    assert_eq!(
        stored_items
            .iter()
            .map(|i| i.kind.as_str())
            .collect::<Vec<_>>(),
        vec!["prompt", "permission", "turn"],
        "拒绝不能在接收或落库前消失"
    );
    assert!(state.retiring.is_empty(), "后台通道断开且排空后必须释放");
}
