#![cfg(unix)]
use norma::codex::relay::{CodexQuestionRelay, CodexRelayEvent};
use norma::protocol::QuestionAnswer;
use serde_json::json;
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::Duration;
#[path = "support/codex_relay.rs"]
mod support;
use support::{read, send, timed};

/// 场景：两个relay实例的首代计数都为1，旧实例的句柄不得用于另一原生连接。
/// 构造：两个独立Unix连接重放完全相同的thread/turn/RPC/source，保留第一实例的句柄。
/// 执行：第二实例先尝试旧句柄，再用自己的句柄提交；服务端只允许后一条Green。
/// 预期：实例身份与附着代数均被校验，内容相同不能替代连接归属。
#[test]
fn a_question_lease_cannot_cross_relay_instances() {
    let mut previous = None;
    for index in 0..2 {
        let dir = tempfile::Builder::new()
            .prefix("nqr-")
            .tempdir_in("/tmp")
            .unwrap();
        let path = dir.path().join("server.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let (native, bridge) = UnixStream::pair().unwrap();
        let (close, gate) = crossbeam_channel::bounded(1);
        let server = std::thread::spawn(move || {
            let mut socket = tungstenite::accept(timed(listener.accept().unwrap().0)).unwrap();
            read(&mut socket);
            send(
                &mut socket,
                json!({"id":"start","result":{"thread":{"id":"codex-question-test-thread"}}}),
            );
            send(
                &mut socket,
                serde_json::from_str(include_str!("fixtures/questions/codex-source.json")).unwrap(),
            );
            send(
                &mut socket,
                serde_json::from_str(include_str!("fixtures/questions/codex-request.json"))
                    .unwrap(),
            );
            if index == 1 {
                assert_eq!(
                    read(&mut socket)["result"]["answers"]["label"]["answers"],
                    json!(["Green"]),
                    "仅当前连接句柄可写出"
                );
                send(
                    &mut socket,
                    serde_json::from_str(include_str!("fixtures/questions/codex-consumed.json"))
                        .unwrap(),
                );
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
            for _ in 0..3 {
                read(&mut socket);
            }
            if index == 1 {
                read(&mut socket);
            }
            gate.recv_timeout(Duration::from_secs(5)).unwrap();
            socket.close(None).unwrap();
        });
        let relay = CodexQuestionRelay::connect(bridge, &path).unwrap();
        let current = loop {
            if let CodexRelayEvent::Question(change) =
                relay.events().recv_timeout(Duration::from_secs(5)).unwrap()
            {
                break change.question;
            }
        };
        if let Some(old) = previous.take() {
            let answers = || {
                vec![QuestionAnswer {
                    key: "question-0".into(),
                    selected: vec!["option-1".into()],
                    text: None,
                }]
            };
            let rejected = relay.answer(old, answers());
            assert!(
                rejected.is_err()
                    || rejected
                        .unwrap()
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap()
                        .is_err(),
                "同为generation=1不能让旧实例句柄写入新连接"
            );
            relay
                .answer(current, answers())
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap();
        } else {
            previous = Some(current);
        }
        close.send(()).unwrap();
        tui.join().unwrap();
        server.join().unwrap();
    }
}
