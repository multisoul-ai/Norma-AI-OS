use super::*;
use crate::app::{
    doubao::playback::start_playback_with,
    voice::playback::AssistantAudioSink,
    voice_window_inset::{TtsPlaybackSource, VoiceWindowInsetEventBus},
};
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tokio_util::sync::CancellationToken;
struct ObservedSink(async_channel::Sender<u8>);
impl AssistantAudioSink for ObservedSink {
    fn write_chunk(&mut self, chunk: &PcmAudioChunk) -> Result<std::time::Duration, String> {
        let _ = self.0.try_send(chunk.bytes[0]);
        Ok(std::time::Duration::from_millis(20))
    }
    fn stop(&mut self) {}
}
fn audio_event(byte: u8) -> Message {
    Message::Text(json!({"type":"session.output_audio.delta","delta":base64::Engine::encode(&base64::engine::general_purpose::STANDARD,vec![byte;640])}).to_string().into())
}
async fn output_state(events: &async_channel::Receiver<CallEvent>, blocked: bool) {
    loop {
        if matches!(events.recv().await.expect("UI事件"),CallEvent::OutputBlocked(value) if value==blocked)
        {
            break;
        }
    }
}

/// AC02/AC06/AC09：实际媒体循环停止输出期间仍上传真PCM，恢复后只播放新音频。
/// 构造：本地WS依次输出0x11/0x22/0x44标记PCM，麦克风在blocked后提供0x33。
/// 执行：首次声音到sink→停止→服务器确认仍收到0x33并发字幕barrier→恢复→新声音→关闭。
/// 预期：sink只有0x11和0x44；0x22从未播放；输入未静音、通话未提前关闭，最终用量确认。
#[tokio::test]
async fn gpt_live_output_stop_keeps_microphone_and_resume_drops_old_audio() {
    tokio::time::timeout(std::time::Duration::from_secs(5),async {
        let listener=TcpListener::bind("127.0.0.1:0").await.expect("本地WS");
        let address=listener.local_addr().expect("地址");
        let server=tokio::spawn(async move {
            let (tcp,_)=listener.accept().await.expect("客户端");
            let mut socket=accept_async(tcp).await.expect("握手");
            let _=socket.next().await.expect("start").expect("完整");
            socket.send(Message::Text(json!({"type":"session.started","session":{"id":"output-loop","model":"gpt-live-1"}}).to_string().into())).await.expect("started");
            socket.send(audio_event(0x11)).await.expect("初始声音");
            loop {let packet=socket.next().await.expect("停止命令").expect("完整");if packet.to_text().expect("文本").contains("宿主已停止本地播报"){break;}}
            socket.send(audio_event(0x22)).await.expect("阻断期间旧音频");
            loop {
                let packet=socket.next().await.expect("麦克风仍上传").expect("完整");
                let packet:Value=serde_json::from_str(packet.to_text().expect("文本")).expect("JSON");
                if let Some(audio)=packet.get("audio").and_then(Value::as_str) {
                    let bytes=base64::Engine::decode(&base64::engine::general_purpose::STANDARD,audio).expect("PCM");
                    if bytes==vec![0x33;640] {break;}
                }
            }
            socket.send(Message::Text(json!({"type":"session.output_transcript.delta","event_id":"barrier","delta":"输入仍然正常","start_ms":0,"end_ms":20}).to_string().into())).await.expect("阻断barrier");
            loop {let packet=socket.next().await.expect("恢复命令").expect("完整");if packet.to_text().expect("文本").contains("宿主已恢复接收新语音"){break;}}
            socket.send(audio_event(0x44)).await.expect("新声音");
            while let Some(Ok(packet))=socket.next().await {if packet.to_text().expect("文本").contains("session.close"){break;}}
            socket.send(Message::Text(json!({"type":"session.closed","reason":"close_requested","session":{"id":"output-loop","model":"gpt-live-1"},"usage":{"seconds":1.0}}).to_string().into())).await.expect("正常关闭");
        });
        let connection=live_voice::connect_loopback(address,live_voice::LiveConfig::default(),"fixture",CancellationToken::new()).await.expect("连接");
        let (bus,_wake)=VoiceWindowInsetEventBus::new();
        let (sink,played)=async_channel::bounded(8);
        let playback=start_playback_with(move ||Box::new(ObservedSink(sink)),bus.sink(TtsPlaybackSource::GptLive));
        let (microphone,captured)=async_channel::bounded(4);
        microphone.send(CaptureEvent::Ready{generation:0}).await.expect("就绪");
        let (capture_mode,_mode)=tokio::sync::watch::channel(CaptureMode{generation:0,muted:false});
        let (commands,controls)=async_channel::bounded(8);
        let (events,visible)=async_channel::bounded(32);
        let cancellation=CancellationToken::new();
        let worker=tokio::spawn(run_audio_loop(connection,CaptureInput {captured,capture_mode,vad:None},playback,controls,events,cancellation.clone()));
        assert_eq!(played.recv().await.expect("首PCM已到sink"),0x11,"初始声音正常");
        commands.send(CallCommand::SetOutputBlocked(true)).await.expect("停止播报");
        output_state(&visible,true).await;
        microphone.send(CaptureEvent::Pcm{generation:0,chunk:PcmAudioChunk::new(audio::PcmFormat::Pcm16,16_000,1,vec![0x33;640]).expect("真PCM")}).await.expect("继续收音");
        loop {if matches!(visible.recv().await.expect("输入确认"),CallEvent::Transcript(part) if part.event_id=="barrier"){break;}}
        assert!(played.is_empty(),"阻断期间0x22必须从未进入sink");
        commands.send(CallCommand::SetOutputBlocked(false)).await.expect("恢复");
        output_state(&visible,false).await;
        assert_eq!(played.recv().await.expect("恢复后PCM"),0x44,"不得补播0x22");
        cancellation.cancel();
        let outcome=worker.await.expect("音频循环");
        assert!(outcome.error.is_none(),"控制不能提前结束通话：{:?}",outcome.error);
        assert!(outcome.usage.finalized,"正常关闭确认最终服务端用量");
        server.await.expect("服务端断言");
    }).await.expect("输出控制集成测试有界");
}

/// 场景：转写中的停播/恢复话术必须交给模型，不能由本地固定短语改变音频输出。
/// 构造：真实回环WS连续发送四条用户转写、标记PCM和字幕barrier；无UI控制命令。
/// 执行：生产run_audio_loop处理转写与播放，观察UI事件及实际sink，再正常关闭。
/// 预期：转写原样转发，零OutputBlocked事件，PCM可播放；不代表真人或云端语义验收。
#[tokio::test]
async fn gpt_live_transcripts_do_not_trigger_keyword_playback_controls() {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("本地WS");
        let address = listener.local_addr().expect("地址");
        let words = ["停一下", "暂停播报", "继续播报", "请停止播报"];
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.expect("客户端");
            let mut socket = accept_async(tcp).await.expect("握手");
            let _ = socket.next().await.expect("start").expect("完整start");
            socket.send(Message::Text(json!({"type":"session.started","session":{"id":"transcript-only","model":"gpt-live-1"}}).to_string().into())).await.expect("started");
            for (index, word) in words.into_iter().enumerate() {
                socket.send(Message::Text(json!({"type":"session.input_transcript.delta","event_id":format!("user-{index}"),"delta":word,"start_ms":index*1000,"end_ms":index*1000+100}).to_string().into())).await.expect("用户转写");
            }
            socket.send(audio_event(0x55)).await.expect("标记PCM");
            socket.send(Message::Text(json!({"type":"session.output_transcript.delta","event_id":"barrier","delta":"原样转发完成","start_ms":4000,"end_ms":4100}).to_string().into())).await.expect("转写barrier");
            while let Some(Ok(packet)) = socket.next().await {
                if packet.to_text().expect("文本").contains("session.close") { break; }
            }
            socket.send(Message::Text(json!({"type":"session.closed","reason":"close_requested","session":{"id":"transcript-only","model":"gpt-live-1"},"usage":{"seconds":1.0}}).to_string().into())).await.expect("关闭确认");
        });
        let connection = live_voice::connect_loopback(address, live_voice::LiveConfig::default(), "fixture", CancellationToken::new()).await.expect("连接");
        let (bus, _wake) = VoiceWindowInsetEventBus::new();
        let (sink, played) = async_channel::bounded(8);
        let playback = start_playback_with(move || Box::new(ObservedSink(sink)), bus.sink(TtsPlaybackSource::GptLive));
        let (_microphone, captured) = async_channel::bounded(4);
        let (capture_mode, _mode) = tokio::sync::watch::channel(CaptureMode { generation: 0, muted: false });
        let (_commands, controls) = async_channel::bounded(4);
        let (events, visible) = async_channel::bounded(32);
        let cancellation = CancellationToken::new();
        let worker = tokio::spawn(run_audio_loop(connection, CaptureInput { captured, capture_mode, vad: None }, playback, controls, events, cancellation.clone()));
        let mut forwarded = Vec::new();
        let mut blocked = Vec::new();
        loop {
            match visible.recv().await.expect("音频循环UI事件") {
                CallEvent::OutputBlocked(value) => blocked.push(value),
                CallEvent::Transcript(part) if part.event_id == "barrier" => break,
                CallEvent::Transcript(part) => forwarded.push(part.delta),
                _ => {},
            }
        }
        let pcm = if blocked.is_empty() { Some(played.recv().await.expect("真实sink接收PCM")) } else { None };
        cancellation.cancel();
        let result = worker.await.expect("音频循环结束");
        server.await.expect("服务端正常关闭");
        assert_eq!(forwarded, words, "所有转写应保持原文交给后续模型");
        assert!(blocked.is_empty(), "转写不能自动触发本地播放状态变化：{blocked:?}");
        assert_eq!(pcm, Some(0x55), "没有显式UI控制时音频必须正常到达sink");
        assert!(result.error.is_none(), "转写和正常关闭不得报错：{:?}", result.error);
        assert!(result.usage.finalized, "必须完成关闭和用量确认");
    }).await.expect("真实WS转写验收必须有界");
}
