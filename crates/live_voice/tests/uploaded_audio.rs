#![cfg(feature = "test-support")]
#[path = "support/live_server.rs"]
mod live_server;
use audio::{PcmAudioChunk, PcmFormat};
use futures::{SinkExt, StreamExt};
use live_voice::{connect_loopback, Acknowledgement, LiveCommand, LiveConfig, LiveEvent};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

fn pcm(byte: u8) -> PcmAudioChunk {
    // 16kHz × 10ms = 160 mono samples = 320 PCM16 bytes.
    PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![byte; 320]).expect("10ms PCM")
}

/// AC03/AC05/AC12：授权VAD只分析实际WS发出的PCM，静音清掉的积压不能推进上传采样时钟。
/// 数据构造：首个10ms包已发；第二包在队列被mute清掉；unmute后再发一个10ms包。
/// 执行过程：订阅上传观察→首包观察→队列包+mute→ACK→unmute+新包→观察→正常关闭。
/// 预期结果：上传观察只包含真实两个包，样本起点为0和160；generation变化且时轴不倒退。
#[tokio::test]
async fn uploaded_sample_clock_excludes_audio_discarded_at_mute_boundary() {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("本地服务");
        let address = listener.local_addr().expect("地址");
        let server = tokio::spawn(async move {
            let mut socket = live_server::ready(listener, "live_upload_clock").await;
            for expected in [
                "session.input_audio.append",
                "session.input_audio.mute",
                "session.input_audio.unmute",
                "session.input_audio.append",
            ] {
                let message = socket.next().await.expect("事件").expect("完整事件");
                let event: Value =
                    serde_json::from_str(message.to_text().expect("文本")).expect("JSON");
                assert_eq!(event["type"], expected, "静音边界不能泄漏排队音频");
                if expected == "session.input_audio.mute" {
                    socket
                        .send(Message::Text(
                            json!({"type":"session.input_audio.muted","client_event_id":"mute"})
                                .to_string()
                                .into(),
                        ))
                        .await
                        .expect("mute ACK");
                }
            }
            live_server::finish(&mut socket, "live_upload_clock").await;
        });
        let mut client = connect_loopback(
            address,
            LiveConfig::default(),
            "fixture-key",
            CancellationToken::new(),
        )
        .await
        .expect("Live连接");
        let control = client.control();
        let mut uploaded = control.observe_uploaded_audio().expect("唯一上传观察者");
        assert!(
            control.observe_uploaded_audio().is_err(),
            "不能替换已有观察者导致漏边界"
        );
        control.send_audio(pcm(0x11)).expect("首包");
        let first = uploaded.recv().await.expect("实际发送首包");
        assert_eq!(first.start_sample, 0, "上传从0起算");
        assert_eq!(first.pcm, pcm(0x11), "观察保留实际发送的PCM");
        drop(first);
        control.send_audio(pcm(0x33)).expect("排队包");
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
            "先确认mute"
        );
        control
            .send_command(&LiveCommand::Mute {
                event_id: "unmute".into(),
                muted: false,
            })
            .expect("恢复");
        control.send_audio(pcm(0x22)).expect("新包");
        let second = uploaded.recv().await.expect("实际新包");
        assert_eq!(second.start_sample, 160, "被丢弃的包不计入上传时间轴");
        assert_eq!(second.pcm, pcm(0x22), "只观察unmute后新PCM");
        assert_eq!(second.input_generation, 2, "mute/unmute推进两次generation");
        drop(second);
        assert!(uploaded.try_recv().is_err(), "丢弃的0x33没有上传观察回执");
        assert!(client.finish().await.error.is_none(), "观察不妨碍有界关闭");
        server.await.expect("上游断言通过");
    })
    .await
    .expect("上传观察测试有界");
}

/// AC11：会话结束后即使调用方仍保留control，上传观察队列也必须终止并释放许可。
/// 数据构造：真实loopback WS，保留control与唯一观察者；服务端按正常close返回最终用量。
/// 执行过程：finish→在保留control期间等待观察通道终态。
/// 预期结果：recv立即返回None，input budget已释放；不遗留等待未来PCM的后台消费者。
#[tokio::test]
async fn uploaded_observer_closes_even_when_control_outlives_the_session() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("本地服务");
    let address = listener.local_addr().expect("地址");
    let server = tokio::spawn(async move {
        let mut socket = live_server::ready(listener, "live_observer_close").await;
        live_server::finish(&mut socket, "live_observer_close").await;
    });
    let client = connect_loopback(
        address,
        LiveConfig::default(),
        "fixture-key",
        CancellationToken::new(),
    )
    .await
    .expect("连接");
    let control = client.control();
    let mut uploaded = control.observe_uploaded_audio().expect("观察者");
    let outcome = client.finish().await;
    assert!(outcome.error.is_none(), "正常关闭成功");
    let terminal = tokio::time::timeout(std::time::Duration::from_secs(1), uploaded.recv()).await;
    assert!(
        matches!(terminal, Ok(None)),
        "driver结束必须关闭观察队列，不能等control释放"
    );
    assert_eq!(control.queued_audio_bytes(), 0, "终态不残留input许可");
    assert!(
        control.observe_uploaded_audio().is_err(),
        "已结束的control不能重新订阅"
    );
    server.await.expect("上游完整关闭");
}

/// AC03/AC12：隐私静音下可单独维持全零PCM节拍，不能从真实麦克风数据构造可泄漏的上传。
/// 数据构造：先mute，真实0x44语音被过滤，再请求160样本的10ms静音帧。
/// 执行过程：上游验证mute→全零append；客户端核对上传观察并正常结束。
/// 预期结果：只上传320B全零音频，时钟可推进；无效样本数被拒绝。
#[tokio::test]
async fn muted_microphone_can_keep_a_synthetic_silence_clock() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("本地服务");
    let address = listener.local_addr().expect("地址");
    let server = tokio::spawn(async move {
        let mut socket = live_server::ready(listener, "live_silent_clock").await;
        let mute = socket.next().await.expect("mute").expect("完整mute");
        assert!(
            mute.to_text()
                .expect("文本")
                .contains("session.input_audio.mute"),
            "先静音"
        );
        let frame = socket.next().await.expect("静音帧").expect("完整帧");
        let frame: Value = serde_json::from_str(frame.to_text().expect("文本")).expect("JSON");
        assert_eq!(
            frame["type"], "session.input_audio.append",
            "持续上传静音节拍"
        );
        let bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            frame["audio"].as_str().expect("音频"),
        )
        .expect("base64");
        assert_eq!(bytes, vec![0; 320], "静音时绝不能上传真实麦克风样本");
        live_server::finish(&mut socket, "live_silent_clock").await;
    });
    let client = connect_loopback(
        address,
        LiveConfig::default(),
        "fixture-key",
        CancellationToken::new(),
    )
    .await
    .expect("连接");
    let control = client.control();
    let mut observed = control.observe_uploaded_audio().expect("观察者");
    control
        .send_command(&LiveCommand::Mute {
            event_id: "mute".into(),
            muted: true,
        })
        .expect("静音");
    control
        .send_audio(pcm(0x44))
        .expect("真实语音由隐私gate过滤");
    assert!(control.send_silence(0).is_err(), "空帧无效");
    assert!(control.send_silence(16001).is_err(), "超过一秒的静音帧无效");
    control.send_silence(160).expect("仅生成10ms全零帧");
    let frame = observed.recv().await.expect("实际静音上传");
    assert_eq!(frame.pcm, pcm(0), "上传观察只含全零PCM");
    assert_eq!(frame.input_generation, 1, "属于mute之后的generation");
    drop(frame);
    assert!(client.finish().await.error.is_none(), "持续静音后正常关闭");
    server.await.expect("上游断言通过");
}
