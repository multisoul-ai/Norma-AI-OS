use crate::app::MultisoulApp;
use audio::{PcmAudioChunk, PcmFormat};
use base64::Engine as _;
use futures::{SinkExt, StreamExt};
use gpui::{Context, Window};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::PathBuf;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, http::HeaderValue, Message};
use tokio_util::sync::CancellationToken;

mod playback;
mod tools;

use playback::{start_playback, AudioTurnGate};

const DOUBAO_URL: &str = "wss://openspeech.bytedance.com/api/v3/duplex/realtime/dialogue";
const DOUBAO_VOICE: &str = "zh_female_vv_jupiter_bigtts";
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

fn connect_failure_message(error: &tokio_tungstenite::tungstenite::Error) -> &'static str {
    match error {
        tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == 401 => {
            "豆包语音服务拒绝了 API Key；请在豆包语音控制台创建全双工可用的 Key"
        }
        tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == 403 => {
            "豆包账号尚未获得全双工访问权限"
        }
        _ => "豆包连接失败，请检查网络和全双工权限",
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DoubaoStatus {
    Idle,
    Connecting,
    Connected,
    Disconnected(String),
}

pub(crate) struct DoubaoVoiceState {
    pub(crate) status: DoubaoStatus,
    pub(crate) generation: u64,
    pub(crate) cancellation: Option<CancellationToken>,
    pub(crate) reports: Option<async_channel::Sender<String>>,
    pub(crate) tracked_tasks: HashSet<u64>,
}

impl Default for DoubaoVoiceState {
    fn default() -> Self {
        Self {
            status: DoubaoStatus::Idle,
            generation: 0,
            cancellation: None,
            reports: None,
            tracked_tasks: HashSet::new(),
        }
    }
}

impl DoubaoVoiceState {
    pub(crate) fn stop(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if let Some(cancellation) = self.cancellation.take() {
            cancellation.cancel();
        }
        self.reports = None;
        self.tracked_tasks.clear();
        self.status = DoubaoStatus::Idle;
    }

    pub(crate) fn is_active(&self) -> bool {
        self.cancellation.is_some()
    }
}

pub(crate) fn session_create(instructions: &str) -> Value {
    let tools = agent::norma_tool_definitions()
        .into_iter()
        .map(|tool| {
            json!({"type":"function", "name":tool.name,
            "description":tool.description, "parameters":tool.parameters})
        })
        .collect::<Vec<_>>();
    json!({
        "type":"session.create",
        "session": {
            "model":"1.2.6.1", "instructions":instructions,
            "audio": {
                "input":{"format":{"type":"pcm", "rate":16000}},
                "output":{"format":{"type":"pcm", "rate":24000}, "voice":DOUBAO_VOICE}
            },
            "tools":tools
        },
        "extension":{"tts":{"audio_config":{"format":"pcm_s16le", "sample_rate":24000, "channel":1}}}
    })
}

impl MultisoulApp {
    pub(crate) fn queue_doubao_terminal_reports(&mut self) {
        let (Some(sender), Some(coordinator)) = (
            self.doubao.reports.as_ref(),
            self.agent_task_coordinator.as_ref(),
        ) else {
            return;
        };
        for task in coordinator.tasks() {
            if !self.doubao.tracked_tasks.contains(&task.task_id) {
                continue;
            }
            let Some(report) = task_completion_message(&task) else {
                continue;
            };
            if sender.try_send(report).is_ok() {
                self.doubao.tracked_tasks.remove(&task.task_id);
            }
        }
    }

    pub(crate) fn stop_doubao_voice(&mut self, cx: &mut Context<Self>) {
        self.doubao.stop();
        cx.notify();
    }

    pub(crate) fn toggle_doubao_voice(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.doubao.is_active() {
            self.stop_doubao_voice(cx);
            return;
        }
        let Some(account) = self
            .voice_settings
            .values()
            .doubao_credential_ref()
            .map(str::to_owned)
        else {
            self.doubao.status =
                DoubaoStatus::Disconnected("请在 Voice Setup 保存豆包 API Key".into());
            cx.notify();
            return;
        };
        let canvas = self
            .current_canvas_context_for_provider()
            .unwrap_or_else(|| agent::CurrentCanvasContext {
                project_name: "未打开项目".into(),
                workspace_root: "未打开项目；项目操作不可用".into(),
                total_cards: 0,
                cards: Vec::new(),
                conversation_tails: Vec::new(),
            });
        let project_id = self
            .runtime
            .workspace_root()
            .map(|root| root.display().to_string());
        let bound_project = project_id.clone();
        let roster = self.agent_roster_for_provider();
        let prompt = agent::norma_system_prompt(&canvas, &roster);
        self.doubao.generation = self.doubao.generation.wrapping_add(1);
        let generation = self.doubao.generation;
        self.doubao.tracked_tasks.clear();
        let tool_adapter = project_id.as_ref().map(|project| {
            super::agent_runtime::start_doubao_adapter(project.clone(), generation, window, cx)
        });
        let tools = tool_adapter.map(agent::NormaToolRuntime::new);
        let database = self.database_paths.app_db_path().to_path_buf();
        let cancellation = CancellationToken::new();
        self.doubao.cancellation = Some(cancellation.clone());
        self.doubao.status = DoubaoStatus::Connecting;
        let (events_tx, events_rx) = async_channel::bounded(32);
        let (reports_tx, reports_rx) = async_channel::unbounded();
        self.doubao.reports = Some(reports_tx);
        if let Ok(runtime) = super::shared_tokio_runtime() {
            runtime.spawn(async move {
                let result = run_session(
                    account,
                    prompt,
                    project_id,
                    database,
                    tools,
                    cancellation,
                    events_tx.clone(),
                    reports_rx,
                )
                .await;
                let _ = events_tx
                    .send(DoubaoStatus::Disconnected(
                        result.err().unwrap_or_else(|| "通话已结束".into()),
                    ))
                    .await;
            });
        } else {
            self.doubao.stop();
            self.doubao.status = DoubaoStatus::Disconnected("无法启动语音网络运行时".into());
            cx.notify();
            return;
        }
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(100))
                .await;
            let still_active = this
                .update_in(cx, |app, _, cx| {
                    if app.doubao.generation != generation {
                        return false;
                    }
                    let now = app
                        .runtime
                        .workspace_root()
                        .map(|root| root.display().to_string());
                    if now != bound_project {
                        app.stop_doubao_voice(cx);
                        return false;
                    }
                    while let Ok(status) = events_rx.try_recv() {
                        let ended = matches!(status, DoubaoStatus::Disconnected(_));
                        app.doubao.status = status;
                        if ended {
                            app.doubao.cancellation = None;
                            app.doubao.reports = None;
                            app.doubao.tracked_tasks.clear();
                        }
                        cx.notify();
                    }
                    app.doubao.is_active()
                })
                .unwrap_or(false);
            if !still_active {
                break;
            }
        })
        .detach();
        cx.notify();
    }
}

fn task_completion_message(task: &agent::AgentCoordinatedTask) -> Option<String> {
    let outcome = match task.state {
        "succeeded" => "已完成",
        "failed" => "失败",
        "cancelled" => "已取消",
        "interrupted" => "已中断",
        _ => return None,
    };
    Some(format!(
        "{} 的任务 #{} {}。",
        task.agent, task.task_id, outcome
    ))
}

async fn run_session(
    account: String,
    prompt: String,
    project_id: Option<String>,
    database: PathBuf,
    tools: Option<agent::NormaToolRuntime>,
    cancellation: CancellationToken,
    events: async_channel::Sender<DoubaoStatus>,
    reports: async_channel::Receiver<String>,
) -> Result<(), String> {
    let key = tokio::task::spawn_blocking(move || {
        crate::settings::voice::VoiceSettingsState::load_credential(&account)
    })
    .await
    .map_err(|_| "无法读取豆包凭据".to_string())??
    .ok_or_else(|| "豆包 API Key 已丢失，请重新保存".to_string())?;
    #[cfg(feature = "e2e-test-support")]
    let url = std::env::var("NORMA_DOUBAO_WS_URL").unwrap_or_else(|_| DOUBAO_URL.into());
    #[cfg(not(feature = "e2e-test-support"))]
    let url = DOUBAO_URL.to_string();
    let mut request = url
        .into_client_request()
        .map_err(|_| "豆包连接地址无效".to_string())?;
    request.headers_mut().insert(
        "X-Api-Key",
        HeaderValue::from_str(&key).map_err(|_| "豆包 API Key 格式无效".to_string())?,
    );
    let (mut socket, _) =
        tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(request))
            .await
            .map_err(|_| "豆包连接超时，请检查网络".to_string())?
            .map_err(|error| connect_failure_message(&error).to_string())?;
    socket
        .send(Message::Text(session_create(&prompt).to_string().into()))
        .await
        .map_err(|_| "豆包会话配置发送失败".to_string())?;
    let playback = start_playback();
    let (tool_result_tx, tool_result_rx) = async_channel::bounded::<Value>(16);
    let (tool_call_tx, tool_call_rx) = async_channel::bounded::<(Value, String, String, u64)>(16);
    tokio::spawn(async move {
        while let Ok((call, session, transcript, turn)) = tool_call_rx.recv().await {
            let result =
                if let (Some(runtime), Some(project)) = (tools.as_ref(), project_id.as_ref()) {
                    tools::execute_call(
                        runtime,
                        &database,
                        project,
                        &session,
                        &transcript,
                        turn,
                        call,
                    )
                    .await
                } else {
                    tools::output(
                        call["call_id"].as_str().unwrap_or("invalid"),
                        json!({"status":"rejected","message":"请先打开项目"}),
                    )
                };
            if tool_result_tx.send(result).await.is_err() {
                break;
            }
        }
    });
    let mut pcm_session = None;
    let mut pcm_rx: Option<async_channel::Receiver<PcmAudioChunk>> = None;
    let mut user_transcript = String::new();
    let mut transcript_turn = 0_u64;
    let mut provider_session = String::new();
    let mut pcm_buffer = Vec::new();
    let mut user_speaking = false;
    let mut output_active = false;
    let mut audio_gate = AudioTurnGate::default();
    let handshake_deadline = tokio::time::sleep(CONNECT_TIMEOUT);
    tokio::pin!(handshake_deadline);
    let result = 'session: loop {
        tokio::select! {
            _ = &mut handshake_deadline, if provider_session.is_empty() => {
                break Err("豆包会话确认超时，请重试".into());
            }
            _ = cancellation.cancelled() => {
                let _ = socket.send(Message::Text(json!({"type":"session.close"}).to_string().into())).await;
                break Ok(());
            }
            playback_error = playback.errors.recv() => {
                break Err(format!("豆包音频播放失败：{}", playback_error.unwrap_or_else(|_| "输出设备不可用".into())));
            }
            report = reports.recv(), if !provider_session.is_empty() && !user_speaking && !output_active => {
                if let Ok(text) = report {
                    socket.send(Message::Text(json!({"type":"speech_text_buffer.commit","text":text}).to_string().into()))
                        .await.map_err(|_| "豆包任务结果播报中断".to_string())?;
                }
            }
            received = socket.next() => {
                let Some(received) = received else { break Err("豆包连接已断开".into()); };
                let message = received.map_err(|_| "豆包连接已断开".to_string())?;
                let Message::Text(text) = message else { continue; };
                let event: Value = serde_json::from_str(&text).map_err(|_| "豆包返回了无效事件".to_string())?;
                match event["type"].as_str() {
                    Some("session.created") => {
                        let Some(id) = event["session"]["id"].as_str().filter(|id| !id.is_empty()) else { break Err("豆包未返回会话 ID".into()); };
                        provider_session = id.to_string();
                        let (capture, chunks) = tokio::task::spawn_blocking(move || voice_input::start_duplex_pcm_stream(None)).await
                            .map_err(|_| "麦克风启动失败".to_string())?
                            .map_err(|error| format!("麦克风启动失败：{error}"))?;
                        pcm_session = Some(capture);
                        pcm_rx = Some(chunks);
                        let _ = events.send(DoubaoStatus::Connected).await;
                    }
                    Some("conversation.item.input_audio_transcription.completed") => {
                        user_speaking = false;
                        if let Some(text) = event["transcript"].as_str().filter(|text| !text.trim().is_empty()) {
                            transcript_turn = transcript_turn.wrapping_add(1);
                            user_transcript = text.to_string();
                        }
                    }
                    Some("conversation.item.input_audio_transcription.started") | Some("input_audio_buffer.speech_started") => {
                        user_speaking = true;
                        user_transcript.clear();
                        if output_active {
                            socket.send(Message::Text(json!({"type":"response.cancel"}).to_string().into()))
                                .await.map_err(|_| "豆包插话取消失败".to_string())?;
                        }
                        output_active = false;
                        audio_gate.interrupt();
                        playback.interrupt();
                    }
                    Some("conversation.item.input_audio_transcription.failed") => {
                        user_speaking = false;
                        user_transcript.clear();
                    }
                    Some("response.output_audio.started") => {
                        if audio_gate.started(event["response_id"].as_str(), user_speaking) {
                            output_active = true;
                        }
                    }
                    Some("response.output_audio.done") => {
                        if audio_gate.finished(event["response_id"].as_str()) {
                            output_active = false;
                        }
                    }
                    Some("response.done") => {
                        user_transcript.clear();
                        if !audio_gate.waiting_for_next_start && audio_gate.active.is_none() {
                            output_active = false;
                        }
                    }
                    Some("response.output_audio.delta") => {
                        if audio_gate.accept_delta(event["response_id"].as_str(), user_speaking) {
                            output_active = true;
                        } else {
                            continue;
                        }
                        if let Some(delta) = event["delta"].as_str() {
                            let bytes = base64::engine::general_purpose::STANDARD.decode(delta).map_err(|_| "豆包音频格式无效".to_string())?;
                            if !bytes.is_empty() {
                                let chunk = PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, bytes).map_err(|_| "豆包 PCM 音频无效".to_string())?;
                                playback.enqueue(chunk)?;
                            }
                        }
                    }
                    Some("response.function_call_arguments.done") => {
                        if let Some(items) = event["items"].as_array() {
                            for item in items {
                                if tool_call_tx.try_send((item.clone(), provider_session.clone(), user_transcript.clone(), transcript_turn)).is_err() {
                                    break 'session Err("豆包工具队列已满".into());
                                }
                            }
                        }
                    }
                    Some("session.closed") => break Ok(()),
                    Some("error") => break Err("豆包服务拒绝了会话，请检查 API Key 与全双工权限".into()),
                    _ => {}
                }
            }
            chunk = async { pcm_rx.as_ref().expect("PCM stream selected only after session created").recv().await }, if pcm_rx.is_some() => {
                match chunk {
                    Ok(chunk) => {
                        pcm_buffer.extend_from_slice(&chunk.bytes);
                        while pcm_buffer.len() >= 640 {
                            let frame: Vec<u8> = pcm_buffer.drain(..640).collect();
                            let audio = base64::engine::general_purpose::STANDARD.encode(frame);
                            socket.send(Message::Text(json!({"type":"input_audio_buffer.append","audio":audio}).to_string().into()))
                                .await.map_err(|_| "豆包音频上传中断".to_string())?;
                        }
                    }
                    Err(_) => break Err("麦克风采集已停止".into()),
                }
            }
            output = tool_result_rx.recv() => {
                if let Ok(output) = output {
                    socket.send(Message::Text(json!({"type":"conversation.item.create","items":[output]}).to_string().into()))
                        .await.map_err(|_| "豆包工具回执发送失败".to_string())?;
                }
            }
        }
    };
    drop(pcm_rx);
    if let Some(capture) = pcm_session {
        tokio::task::spawn_blocking(move || drop(capture))
            .await
            .ok();
    }
    playback.end();
    result
}

#[cfg(test)]
#[path = "../../tests/unit/doubao_tests.rs"]
mod tests;
