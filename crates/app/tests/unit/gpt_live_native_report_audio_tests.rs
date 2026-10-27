use super::*;
use crate::app::{
    doubao::playback::start_playback_with,
    shared_context::SharedContext,
    voice::playback::AssistantAudioSink,
    voice_window_inset::{TtsPlaybackSource, VoiceWindowInsetEventBus},
};
use agent::context::{ContextPayload, ContextProject, ContextScope, ReportDelivery};
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tokio_util::sync::CancellationToken;

struct ReportSink(async_channel::Sender<()>);
impl AssistantAudioSink for ReportSink {
    fn write_chunk(&mut self, _: &PcmAudioChunk) -> Result<std::time::Duration, String> {
        self.0.try_send(()).unwrap();
        Ok(std::time::Duration::from_millis(20))
    }
    fn stop(&mut self) {}
}

/// 场景：幕僚报告经过真实音频循环和回环 WS，收到 ACK 及音频也不能伪造报告播放归属。
/// 构造：持久报告、受控 sink、本地服务逐段确认并在最终 Commentary 后返回标记 PCM。
/// 执行：发送生产 NativeReport，服务端重组完整正文，收到音频后挂断并读取交付账本。
/// 预期：协议追加 ≤500 字节，原生音频仍工作；保存 Sent/Acknowledged/Unknown，无 Playing/Completed。
#[tokio::test]
async fn gpt_live_native_report_round_trip_keeps_audio_and_delivery_evidence_separate() {
    native_report_round_trip(false).await;
}

/// 场景：拆分报告入口后，排队的停播/恢复不能让旧报告在恢复时补播。
/// 构造：本地 WS、受控音频 sink，入口预塞旧报告和停播/恢复命令。
/// 执行：启动生产音频循环，等待恢复反馈，再发新报告并关闭报告入口。
/// 预期：旧报告 Skipped 且从未上网，新报告正常 ACK/出声，关闭入口不忙循环。
#[tokio::test]
async fn gpt_live_native_report_block_drains_ingress_and_closed_ingress_keeps_audio() {
    native_report_round_trip(true).await;
}

async fn native_report_round_trip(block_backlog: bool) {
    tokio::time::timeout(std::time::Duration::from_secs(8), async {
        let text = "当前任务等待用户确认，尚未执行。".repeat(30);
        let dir = tempfile::tempdir().unwrap();
        let service = SharedContext::new(dir.path().join("context.sqlite"), ContextScope {
            workspace_id: "w".into(), area_id: "a".into(), project: ContextProject::Personal,
            task_id: None, conversation_id: None,
        }).unwrap();
        let saved_text = text.clone();
        let record = service.run(None, move |access| access.report("wire", &saved_text, vec![])).await.unwrap();
        let delivery = service.report_delivery(record.reference.clone()).unwrap();
        delivery.set(ReportDelivery::Queued);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (received, confirmed) = tokio::sync::oneshot::channel();
        let expected_report_id = record.reference.event_id.clone();
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(tcp).await.unwrap();
            let start = socket.next().await.unwrap().unwrap();
            assert!(start.to_text().unwrap().contains("session.start"), "必须走真实 Live 首包");
            socket.send(Message::Text(json!({"type":"session.started","session":{"id":"brain-report","model":"gpt-live-1"}}).to_string().into())).await.unwrap();
            let mut assembled = String::new();
            let mut parts = 0;
            let final_id = loop {
                let packet = socket.next().await.unwrap().unwrap();
                let value: Value = serde_json::from_str(packet.to_text().unwrap()).unwrap();
                if value["type"] == "session.input_audio.append" || value["type"] == "session.instructions.append" { continue; }
                let content = value["content"].as_str().unwrap();
                assert!(content.len() <= 500, "实际网络内容包含标签后仍在预算内");
                let data: Value = serde_json::from_str(content).unwrap();
                assert_eq!(data["event_id"], expected_report_id, "线上每个资料和播报片段必须只属于新报告");
                if value["type"] == "session.commentary.append" {
                    assert_eq!(data["total"], parts, "收到所有正文后才有最终请求");
                    assert_eq!(assembled, text, "真实网络正文完整无截断");
                    let id = value["event_id"].as_str().unwrap().to_owned();
                    socket.send(Message::Text(json!({"type":"session.commentary.appended","client_event_id":id}).to_string().into())).await.unwrap();
                    break id;
                }
                assert_eq!(value["type"], "session.thinking.append", "正文必须走资料追加");
                parts += 1;
                assert_eq!(data["part"], parts, "资料按序追加");
                assembled.push_str(data["text"].as_str().unwrap());
                socket.send(Message::Text(json!({"type":"session.thinking.appended","client_event_id":value["event_id"]}).to_string().into())).await.unwrap();
            };
            socket.send(Message::Text(json!({"type":"session.output_audio.delta","delta":base64::Engine::encode(&base64::engine::general_purpose::STANDARD,vec![0x11_u8;640])}).to_string().into())).await.unwrap();
            received.send(final_id).unwrap();
            loop {
                let packet = socket.next().await.unwrap().unwrap();
                let value: Value = serde_json::from_str(packet.to_text().unwrap()).unwrap();
                if value["type"] == "session.close" { break; }
                assert!(value["type"] != "session.thinking.append" && value["type"] != "session.commentary.append", "最终确认后也不能补发旧报告片段");
            }
            socket.send(Message::Text(json!({"type":"session.closed","reason":"close_requested","session":{"id":"brain-report","model":"gpt-live-1"},"usage":{"seconds":1.0}}).to_string().into())).await.unwrap();
        });
        let connection = live_voice::connect_loopback(address, live_voice::LiveConfig::default(), "fixture", CancellationToken::new()).await.unwrap();
        let (bus, _wake) = VoiceWindowInsetEventBus::new();
        let (written, played) = async_channel::bounded(1);
        let playback = start_playback_with(move || Box::new(ReportSink(written)), bus.sink(TtsPlaybackSource::GptLive));
        let (_microphone, captured) = async_channel::bounded(1);
        let (capture_mode, _mode) = tokio::sync::watch::channel(CaptureMode { generation: 0, muted: true });
        let (commands, controls) = async_channel::bounded(4);
        let (reports, report_input) = async_channel::bounded(8);
        let (events, visible) = async_channel::bounded(64);
        let cancellation = CancellationToken::new();
        let stale = if block_backlog {
            let old = service.run(None, |access| access.report("stale", "旧报告不可补播", vec![])).await.unwrap();
            let handle = service.report_delivery(old.reference.clone()).unwrap();
            handle.set(ReportDelivery::Queued);
            reports.send(super::super::native_report::NativeReport::new("旧报告不可补播".into(), norma::protocol::ReportPriority::Normal, handle.clone(), CancellationToken::new())).await.unwrap();
            commands.send(CallCommand::SetOutputBlocked(true)).await.unwrap();
            commands.send(CallCommand::SetOutputBlocked(false)).await.unwrap();
            Some((old.reference, handle))
        } else { None };
        let worker = tokio::spawn(run_audio_loop(connection, CaptureInput { captured, capture_mode, vad: None }, playback, controls, report_input, events, cancellation.clone()));
        if let Some((reference, handle)) = stale {
            loop { if matches!(visible.recv().await.unwrap(), CallEvent::OutputBlocked(false)) { break; } }
            handle.flush().await.unwrap();
            let snapshot = service.snapshot().await.unwrap();
            assert_eq!(snapshot.state.reports[&reference.event_id].delivery, ReportDelivery::Skipped, "停播必须释放入口积压报告");
            assert!(!snapshot.events.iter().any(|e| matches!(&e.event.payload, ContextPayload::Delivery { report, status: ReportDelivery::Sent | ReportDelivery::Acknowledged, .. } if report == &reference)), "旧报告不能落下发送或确认回执");
        }
        let text = match &record.event.payload { ContextPayload::Report { text } => text.clone(), _ => unreachable!() };
        reports.send(super::super::native_report::NativeReport::new(text, norma::protocol::ReportPriority::Normal, delivery.clone(), CancellationToken::new())).await.unwrap();
        drop(reports);
        let final_id = confirmed.await.unwrap();
        loop { if matches!(visible.recv().await.unwrap(), CallEvent::AppendAcknowledged(id) if id == final_id) { break; } }
        played.recv().await.expect("原生音频应真正进入受控 sink");
        delivery.flush().await.unwrap();
        let snapshot = service.snapshot().await.unwrap();
        assert_eq!(snapshot.state.reports[&record.reference.event_id].delivery, ReportDelivery::Unknown, "收到音频仍不能证明属于这份报告");
        assert!(!snapshot.events.iter().any(|e| matches!(e.event.payload, ContextPayload::Delivery { status: ReportDelivery::Playing | ReportDelivery::Completed, .. })), "不能凭 ACK 或无归属音频伪造播放证据");
        cancellation.cancel();
        let outcome = worker.await.unwrap();
        assert!(outcome.error.is_none() && outcome.usage.finalized, "报告追加不能破坏正常挂断");
        server.await.unwrap();
    }).await.expect("原生报告网络集成必须有界");
}
