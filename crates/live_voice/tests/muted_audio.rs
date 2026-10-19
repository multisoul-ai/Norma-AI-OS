#![cfg(feature = "test-support")]

#[path = "support/live_server.rs"]
mod live_server;

use audio::{PcmAudioChunk, PcmFormat};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use futures::{SinkExt, StreamExt};
use live_voice::{connect_loopback, Acknowledgement, LiveCommand, LiveConfig, LiveEvent};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

fn pcm(byte: u8) -> PcmAudioChunk {
    // 16000 samples/s × 2 bytes/sample × 0.01s = 320 bytes.
    PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![byte; 320]).expect("10ms PCM")
}

/// AC03/AC12：静音期间采集的PCM不得在解除静音后排队泄漏。
///
/// 数据构造：0x33为静音前积压，0x11为静音期间PCM，0x22为恢复后的新PCM，每包10ms。
/// 执行过程：积压→立即mute，ACK后同步入队静音PCM→unmute→新PCM，中间不yield。
/// 预期结果：服务端只收到0x22，旧积压和静音期间PCM不发送；会话仍正常关闭。
#[tokio::test]
async fn captured_muted_audio_cannot_leak_after_unmute() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("本地服务");
        let address = listener.local_addr().expect("本地地址");
        let (verified, verification) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let mut socket = live_server::ready(listener, "live_muted_audio").await;
            let message = socket.next().await.expect("mute").expect("mute完整");
            let mute: Value =
                serde_json::from_str(message.to_text().expect("mute文本")).expect("mute JSON");
            assert_eq!(mute["type"], "session.input_audio.mute", "先确认静音");
            socket
                .send(Message::Text(
                    json!({"type":"session.input_audio.muted","client_event_id":"mute"})
                        .to_string()
                        .into(),
                ))
                .await
                .expect("静音ACK");
            let message = socket.next().await.expect("unmute").expect("unmute完整");
            let unmute: Value =
                serde_json::from_str(message.to_text().expect("unmute文本")).expect("unmute JSON");
            assert_eq!(unmute["type"], "session.input_audio.unmute", "恢复输入");
            let message = socket.next().await.expect("恢复后的PCM").expect("PCM完整");
            let event: Value =
                serde_json::from_str(message.to_text().expect("PCM文本")).expect("PCM JSON");
            assert_eq!(
                event["type"], "session.input_audio.append",
                "恢复后继续上传音频"
            );
            let bytes = STANDARD
                .decode(event["audio"].as_str().expect("base64音频"))
                .expect("有效PCM");
            assert_eq!(
                bytes,
                vec![0x22; 320],
                "静音期间0x11绝不能在unmute后送入模型"
            );
            verified.send(()).expect("客户端仍在等待验收");
            live_server::finish(&mut socket, "live_muted_audio").await;
        });
        let mut client = connect_loopback(
            address,
            LiveConfig::default(),
            "fixture-key",
            CancellationToken::new(),
        )
        .await
        .expect("ready");
        let control = client.control();
        control.send_audio(pcm(0x33)).expect("静音前积压");
        control
            .send_command(&LiveCommand::Mute {
                event_id: "mute".into(),
                muted: true,
            })
            .expect("静音");
        assert_eq!(
            client.next_event().await,
            Some(LiveEvent::Acknowledged {
                client_event_id: "mute".into(),
                kind: Acknowledgement::InputMuted
            }),
            "确认静音生效"
        );
        control.send_audio(pcm(0x11)).expect("静音采集由本地过滤");
        control
            .send_command(&LiveCommand::Mute {
                event_id: "unmute".into(),
                muted: false,
            })
            .expect("恢复");
        control.send_audio(pcm(0x22)).expect("新PCM");
        verification.await.expect("实际WS只能收到新PCM");
        let outcome = client.finish().await;
        assert!(outcome.error.is_none(), "隐私过滤后仍应正常关闭");
        server.await.expect("服务端完整退出");
    })
    .await
    .expect("静音顺序测试必须有界");
}
