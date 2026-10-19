#![cfg(feature = "test-support")]

#[path = "support/live_server.rs"]
mod live_server;

use futures::{SinkExt, StreamExt};
use live_voice::{
    connect_loopback, Acknowledgement, AppendKind, DelegationNotice, LiveCommand, LiveConfig,
    LiveEvent,
};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

/// AC05/AC10：真实WS上回传委派结果与静音命令，ACK关联原请求而不关闭会话。
///
/// 数据构造：d1由假服务声明；result_1和mute_1分别对应结果和输入静音。
/// 执行过程：接通知→回传事实→收ACK→静音→收ACK→优雅关闭。
/// 预期结果：wire字段与官方协议一致，没有response.create、任务取消或播放完成假设。
#[tokio::test]
async fn backend_results_and_mute_commands_round_trip_on_the_live_socket() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("应绑定");
        let address = listener.local_addr().expect("应有地址");
        let server = tokio::spawn(async move {
            let mut socket = live_server::ready(listener, "live_controls").await;
            socket.send(Message::Text(json!({"type":"session.delegation.created","offset_ms":1000,"delegation":{"id":"d1","type":"delegation","target":"client"}}).to_string().into()))
                .await.expect("应发送委派");
            let update = socket.next().await.expect("应有结果消息").expect("结果消息有效");
            let value: Value = serde_json::from_str(update.to_text().expect("结果为JSON文本")).expect("结果应有效");
            assert_eq!(value["type"], "session.commentary.append", "不能使用Realtime工具回传协议");
            assert_eq!(value["delegation_id"], "d1", "必须关联原委派");
            assert_eq!(value["content"], "任务已接收，仍在运行", "按真实状态说话");
            socket.send(Message::Text(json!({"type":"session.commentary.appended","client_event_id":"result_1"}).to_string().into()))
                .await.expect("结果ACK应可发送");
            let mute = socket.next().await.expect("应有静音命令").expect("静音有效");
            let value: Value = serde_json::from_str(mute.to_text().expect("静音为文本")).expect("静音JSON有效");
            assert_eq!(value["type"], "session.input_audio.mute", "静音不能取消后台任务");
            socket.send(Message::Text(json!({"type":"session.input_audio.muted","client_event_id":"mute_1"}).to_string().into()))
                .await.expect("静音ACK应可发送");
            live_server::finish(&mut socket, "live_controls").await;
        });
        let mut client = connect_loopback(address, LiveConfig::default(), "fixture-key", CancellationToken::new()).await.expect("应ready");
        assert_eq!(client.next_event().await, Some(LiveEvent::Delegation(DelegationNotice { id: "d1".into(), offset_ms:1000.0 })), "委派通知进入应用层");
        let control = client.control();
        control.send_command(&LiveCommand::Append { event_id: "result_1".into(), kind: AppendKind::Commentary, delegation_id: Some("d1".into()), content: "任务已接收，仍在运行".into() }).expect("应能回传后台结果");
        assert_eq!(client.next_event().await, Some(LiveEvent::Acknowledged { client_event_id:"result_1".into(), kind:Acknowledgement::Appended(AppendKind::Commentary) }), "ACK关联结果更新");
        control.send_command(&LiveCommand::Mute { event_id:"mute_1".into(), muted:true }).expect("应能静音");
        assert_eq!(client.next_event().await, Some(LiveEvent::Acknowledged { client_event_id:"mute_1".into(), kind:Acknowledgement::InputMuted }), "ACK关联静音命令");
        let outcome = client.finish().await;
        assert!(outcome.error.is_none(), "结果回传和静音之后仍能正常结束会话");
        assert!(outcome.usage.finalized, "关闭确认应由closed而不是命令ACK提供");
        server.await.expect("假上游全部断言通过");
    }).await.expect("控制通道测试必须有界");
}
