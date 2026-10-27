#![cfg(feature = "test-support")]
#[path = "support/live_server.rs"]
mod live_server;
use futures::{SinkExt, StreamExt};
use live_voice::{connect_loopback, AppendKind, LiveCommand, LiveConfig, LiveEvent};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

/// 场景：报告已进入本地网络队列但未写 socket 时，取消不能留下迟到播报请求。
/// 构造：单线程 Tokio 与真实回环 WS，在不 yield 的同一轮提交旧报告、取消、提交正常屏障。
/// 执行：服务端检查第一条实际收到的 append，随后确认屏障并正常关闭。
/// 预期：服务端只收到屏障，旧报告不出现在网络上；会话仍可继续并获得关闭回执。
#[tokio::test]
async fn cancelled_scoped_append_is_removed_before_socket_write() {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut socket = live_server::ready(listener, "scoped").await;
            let packet = socket.next().await.unwrap().unwrap();
            let value: Value = serde_json::from_str(packet.to_text().unwrap()).unwrap();
            assert_eq!(value["event_id"], "barrier", "已取消的队列追加不得写上网");
            socket.send(Message::Text(json!({"type":"session.thinking.appended","client_event_id":"barrier"}).to_string().into())).await.unwrap();
            live_server::finish(&mut socket, "scoped").await;
        });
        let mut client = connect_loopback(address, LiveConfig::default(), "fixture", CancellationToken::new()).await.unwrap();
        let control = client.control();
        let scope = CancellationToken::new();
        control.send_scoped_append(&LiveCommand::Append { event_id: "cancelled-report".into(), kind: AppendKind::Commentary, delegation_id: None, content: "旧报告".into() }, &scope).unwrap();
        scope.cancel();
        control.send_command(&LiveCommand::Append { event_id: "barrier".into(), kind: AppendKind::Thinking, delegation_id: None, content: "正常资料".into() }).unwrap();
        let event = client.next_event().await;
        assert!(matches!(event, Some(LiveEvent::Acknowledged { client_event_id, .. }) if client_event_id == "barrier"), "连接应保持可用且收到真实屏障 ACK");
        let outcome = client.finish().await;
        assert!(outcome.error.is_none() && outcome.usage.finalized, "取消局部追加不能关闭或损坏通话");
        server.await.unwrap();
    }).await.expect("网络取消测试必须有界");
}

/// 场景：慢 writer 尚未排空报告时，报告必须给基本通话控制预留容量。
/// 构造：16 槽真实传输队列，同一调度轮填入 12 条可取消追加，再尝试第 13 条。
/// 执行：确认报告退压后提交 4 条静音控制，取消报告并等待服务端逐条确认控制。
/// 预期：报告不能占用最后 4 槽；基本控制均可接收，未发送报告不会抢先上网。
#[tokio::test]
async fn scoped_appends_reserve_capacity_for_call_controls() {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut socket = live_server::ready(listener, "reserved").await;
            for index in 0..4 {
                let packet = socket.next().await.unwrap().unwrap();
                let value: Value = serde_json::from_str(packet.to_text().unwrap()).unwrap();
                assert_eq!(value["event_id"], format!("control-{index}"), "报告不能抢占已取消的发送位置");
                socket.send(Message::Text(json!({"type":"session.input_audio.muted","client_event_id":format!("control-{index}")}).to_string().into())).await.unwrap();
            }
            live_server::finish(&mut socket, "reserved").await;
        });
        let mut client = connect_loopback(address, LiveConfig::default(), "fixture", CancellationToken::new()).await.unwrap();
        let control = client.control();
        let scope = CancellationToken::new();
        for index in 0..13 {
            let result = control.send_scoped_append(&LiveCommand::Append { event_id: format!("report-{index}"), kind: AppendKind::Thinking, delegation_id: None, content: "资料".into() }, &scope);
            if index < 12 { assert!(result.is_ok(), "报告可使用非预留容量"); }
            else { assert_eq!(result, Err(live_voice::LiveError::Capacity), "第十三条报告必须退压，给控制保留四槽"); }
        }
        for index in 0..4 {
            assert!(control.send_command(&LiveCommand::Mute { event_id: format!("control-{index}"), muted: true }).is_ok(), "基本控制必须能入队");
        }
        scope.cancel();
        for index in 0..4 {
            assert!(matches!(client.next_event().await, Some(LiveEvent::Acknowledged { client_event_id, .. }) if client_event_id == format!("control-{index}")), "必须收到控制的真实 ACK");
        }
        let outcome = client.finish().await;
        assert!(outcome.error.is_none() && outcome.usage.finalized, "报告退压不应关闭通话");
        server.await.unwrap();
    }).await.expect("控制预留测试必须有界");
}
