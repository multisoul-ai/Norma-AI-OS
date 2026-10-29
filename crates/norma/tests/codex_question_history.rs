#![cfg(unix)]
use norma::codex::native_questions::{CodexNativeEvent, CodexQuestionHost};
use norma::codex::questions::CodexQuestions;
use norma::codex::relay::CodexQuestionRelay;
use norma::protocol::{QuestionAnswer, QuestionState};
use serde_json::{json, Value};
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::{Duration, Instant};
#[path = "support/codex_relay.rs"]
mod support;
use support::{read, send, timed};

fn frame(index: usize, kind: &str) -> Value {
    let mut value: Value = serde_json::from_str(match kind {
        "source" => include_str!("fixtures/questions/codex-source.json"),
        "request" => include_str!("fixtures/questions/codex-request.json"),
        _ => include_str!("fixtures/questions/codex-consumed.json"),
    })
    .unwrap();
    value["params"]["turnId"] = format!("turn-{index}").into();
    if kind == "request" {
        value["id"] = index.into();
        value["params"]["itemId"] = format!("call-{index}").into();
    } else {
        value["params"]["item"]["call_id"] = format!("call-{index}").into();
    }
    value
}
fn turn(index: usize, state: &str) -> Value {
    json!({"method":format!("turn/{state}"),"params":{"threadId":"codex-question-test-thread","turn":{"id":format!("turn-{index}")}}})
}
fn answer() -> Vec<QuestionAnswer> {
    vec![QuestionAnswer {
        key: "question-0".into(),
        selected: vec!["option-1".into()],
        text: None,
    }]
}

/// 场景：长会话中已完成的普通工具与回合不应耗尽当前问答容量，也不得重开远古题目。
/// 构造：140个已完成回合，每轮一个64KiB普通工具来源；总历史超过128条/8MiB两道上限。
/// 执行：逐回合输入start/source/completed，接着登记新题，再重放第0轮的题目和来源。
/// 预期：新题可以回答且只出现一次；不因历史停用，也不因滚动回收重新接受旧回合。
#[test]
fn completed_history_releases_capacity_without_reopening_old_turns() {
    let mut ledger = CodexQuestions::new("codex-question-test-thread".into());
    for index in 0..140 {
        ledger.observe(&turn(index, "started")).unwrap();
        let mut source = frame(index, "source");
        source["params"]["item"]["name"] = "exec_command".into();
        source["params"]["item"]["arguments"] = "x".repeat(64 * 1024).into();
        assert!(
            ledger.observe(&source).is_ok(),
            "已完成历史不消耗新轮来源容量，index={index}"
        );
        assert!(
            ledger.observe(&turn(index, "completed")).is_ok(),
            "普通回合结束不关闭观察器，index={index}"
        );
    }
    ledger.observe(&turn(140, "started")).unwrap();
    ledger.observe(&frame(140, "source")).unwrap();
    ledger.observe(&frame(140, "request")).unwrap();
    let changes = ledger.take_changes();
    assert_eq!(changes.len(), 1, "长会话末尾仍能登记原题");
    ledger.observe(&frame(0, "source")).unwrap();
    ledger.observe(&frame(0, "request")).unwrap();
    assert!(
        ledger.take_changes().is_empty(),
        "回收历史不使远古回合重新可答"
    );
    assert!(
        ledger.reply(&changes[0].question, &answer()).is_ok(),
        "当前轮次原题仍然可答"
    );
}

/// 场景：同一原生会话连续130次问答，宿主等待列表只能计算当前未完成请求。
/// 构造：同一Unix TUI/server、不同turn/call/RPC的真实问答夹具；130比128容量多2轮。
/// 执行：每题经同进程Hook ticket回答，再让原server返回精确工具消费和回合完成。
/// 预期：全部130题逐一Answered，宿主不被已完成ticket误判饱和，原TUI每轮都收到结果。
#[test]
fn host_releases_consumed_tickets_across_long_session() {
    let dir = tempfile::Builder::new()
        .prefix("nqh-")
        .tempdir_in("/tmp")
        .unwrap();
    let path = dir.path().join("s.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (native, bridge) = UnixStream::pair().unwrap();
    let (close_tx, close_rx) = crossbeam_channel::bounded(1);
    let server = std::thread::spawn(move || {
        let mut socket = tungstenite::accept(timed(listener.accept().unwrap().0)).unwrap();
        read(&mut socket);
        send(
            &mut socket,
            json!({"id":"start","result":{"thread":{"id":"codex-question-test-thread"}}}),
        );
        for index in 0..130 {
            send(&mut socket, turn(index, "started"));
            send(&mut socket, frame(index, "source"));
            send(&mut socket, frame(index, "request"));
            assert_eq!(
                read(&mut socket),
                json!({"id":index,"result":{"answers":{"label":{"answers":["Green"]}}}}),
                "每轮只提交一次原RPC，index={index}"
            );
            send(&mut socket, frame(index, "consumed"));
            send(&mut socket, turn(index, "completed"));
        }
        let _ = socket.read();
    });
    let tui = std::thread::spawn(move || {
        let mut socket = tungstenite::client("ws://localhost/", timed(native))
            .unwrap()
            .0;
        send(
            &mut socket,
            json!({"id":"start","method":"thread/start","params":{}}),
        );
        read(&mut socket);
        for index in 0..130 {
            for _ in 0..3 {
                read(&mut socket);
            }
            assert_eq!(
                read(&mut socket)["params"]["item"]["call_id"],
                format!("call-{index}"),
                "原TUI收到自己的消费"
            );
            read(&mut socket);
        }
        close_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let _ = socket.close(None);
    });
    let host = CodexQuestionHost::start(
        CodexQuestionRelay::connect(bridge, &path).unwrap(),
        7,
        "long-run".into(),
    )
    .unwrap();
    let mut count = 0;
    while count < 130 {
        match host
            .events()
            .recv_timeout(Duration::from_secs(5))
            .expect("长会话宿主不能停止发事件")
        {
            CodexNativeEvent::Question(ticket) => {
                assert!(
                    ticket
                        .submit_delivery(&answer(), &format!("delivery-{count}"), Instant::now())
                        .unwrap(),
                    "本轮首次提交必须生效"
                );
            }
            CodexNativeEvent::Changed(change) if change.state == QuestionState::Answered => {
                count += 1
            }
            CodexNativeEvent::Unavailable => panic!("已消费历史不能停用宿主，count={count}"),
            _ => {}
        }
    }
    assert_eq!(count, 130, "全部原题均有消费回执");
    assert!(host.is_available(), "历史不会占用当前等待容量");
    close_tx.send(()).unwrap();
    tui.join().unwrap();
    server.join().unwrap();
}

/// 场景：回合结束早于精确消费时须保留Unknown，迟到消费完成后必须释放旧请求容量。
/// 构造：130轮原题，每轮先发送答案、结束回合，再补同call的工具结果。
/// 执行：逐轮验证Unknown→Answered及证据，跨过128条容量后仍可接新题。
/// 预期：回执不丢失；已经对账完成的旧条目不永久占有未决请求槽位。
#[test]
fn late_consumption_reclaims_retired_unknown_requests() {
    let mut ledger = CodexQuestions::new("codex-question-test-thread".into());
    for index in 0..130 {
        ledger.observe(&turn(index, "started")).unwrap();
        ledger.observe(&frame(index, "source")).unwrap();
        ledger
            .observe(&frame(index, "request"))
            .expect("迟到消费完成后必须释放旧槽位");
        let question = ledger.take_changes().remove(0).question;
        ledger.reply(&question, &answer()).unwrap();
        ledger.sent(&question);
        ledger.take_changes();
        ledger.observe(&turn(index, "completed")).unwrap();
        assert_eq!(
            ledger.take_changes()[0].state,
            QuestionState::Unknown,
            "回合结束不是消费证据"
        );
        ledger.observe(&frame(index, "consumed")).unwrap();
        assert_eq!(
            ledger.take_changes()[0].state,
            QuestionState::Answered,
            "必须保留迟到精确消费"
        );
    }
}

/// 场景：未知结果保留原来源用于对账，这部分正文仍必须计入8MiB账本预算。
/// 构造：每轮来源附加900KiB兼容扩展字段，最多12轮；10×900KiB已经超过8MiB。
/// 执行：每轮提交后结束但不返回消费，检查预算停止接收新来源的时点。
/// 预期：不晚于第10个来源被拒绝，不能因从sources转移到question而漏记保留内存。
#[test]
fn retained_unknown_source_still_counts_against_ledger_budget() {
    let mut ledger = CodexQuestions::new("codex-question-test-thread".into());
    let mut rejected = None;
    for index in 0..12 {
        ledger.observe(&turn(index, "started")).unwrap();
        let mut source = frame(index, "source");
        source["params"]["extension"] = "x".repeat(900 * 1024).into();
        if ledger.observe(&source).is_err() {
            rejected = Some(index);
            break;
        }
        ledger.observe(&frame(index, "request")).unwrap();
        let question = ledger.take_changes().remove(0).question;
        ledger.reply(&question, &answer()).unwrap();
        ledger.take_changes();
        ledger.observe(&turn(index, "completed")).unwrap();
        ledger.take_changes();
    }
    assert!(
        rejected.is_some_and(|index| index <= 9),
        "保留Unknown来源不能逃逸8MiB预算，rejected={rejected:?}"
    );
}
