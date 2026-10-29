//! 启动豆包会话：播报通道、对话与按需收音共用同一个会话任务。
use super::channel::ChannelBinding;
use super::{input, plan::run_session, ContextRequest, ContextRequests, DoubaoStatus};
use crate::app::MultisoulApp;
use gpui::{Context, Window};
use std::time::Instant;
use tokio_util::sync::CancellationToken;
use ui::i18n::LocalizedText;

impl MultisoulApp {
    pub(super) fn start_doubao_context_requests(
        &mut self,
        generation: u64,
        project_id: Option<String>,
        cx: &mut Context<Self>,
    ) -> ContextRequests {
        let (requests, incoming): (ContextRequests, _) = async_channel::bounded(1);
        let scope = self
            .shared_context_service()
            .ok()
            .map(|service| service.scope().clone());
        // 实体级任务：项目转入后台、窗口关闭后仍能回答模型的上下文请求。
        cx.spawn(async move |this, cx| {
            while let Ok(request) = incoming.recv().await {
                let keep = this
                    .update(cx, |app, cx| {
                        let current_project = app
                            .runtime
                            .workspace_root()
                            .map(|root| root.display().to_string());
                        if app.doubao.generation != generation
                            || current_project != project_id
                            || scope.as_ref().is_none_or(|scope| {
                                !app.shared_context_service()
                                    .is_ok_and(|current| current.scope() == scope)
                            })
                        {
                            return false;
                        }
                        match request {
                            ContextRequest::Question {
                                transcript,
                                cancellation,
                                response,
                            } => {
                                let result =
                                    if cancellation.is_cancelled() || !app.doubao.is_active() {
                                        Err("本次语音已取消，未委托代答。".into())
                                    } else {
                                        app.question_delegation_from_voice(
                                            &transcript,
                                            Some(cancellation),
                                        )
                                        .unwrap_or_else(|| Err("请完整说明本次原题委托。".into()))
                                    };
                                let _ = response.send(result);
                                cx.notify();
                            }
                            ContextRequest::Model(response) => {
                                let canvas = app
                                    .current_canvas_context_for_provider()
                                    .unwrap_or_else(|| agent::CurrentCanvasContext {
                                        project_name: "未打开项目".into(),
                                        workspace_root: "未打开项目；项目操作不可用".into(),
                                        total_cards: 0,
                                        cards: Vec::new(),
                                        conversation_tails: Vec::new(),
                                    });
                                let desktop = app.desktop_context_for_provider(cx);
                                let _ = response.send((
                                    canvas,
                                    app.agent_roster_for_provider(),
                                    desktop,
                                ));
                            }
                        }
                        true
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        })
        .detach();
        requests
    }

    /// 启动播报通道；不打开麦克风。返回 false 表示缺少凭据或运行时。
    pub(crate) fn start_doubao_channel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.doubao.is_active() {
            return true;
        }
        let Some(account) = self
            .voice_settings
            .values()
            .doubao_credential_ref()
            .map(str::to_owned)
        else {
            self.doubao.status =
                DoubaoStatus::Disconnected(LocalizedText::new("doubao.key_required"));
            cx.notify();
            return false;
        };
        let project_id = self
            .runtime
            .workspace_root()
            .map(|root| root.display().to_string());
        self.doubao.generation = self.doubao.generation.wrapping_add(1);
        self.doubao.last_transcript = None;
        self.doubao.last_reply = None;
        self.doubao.blocked = false;
        self.doubao.retry_at = None;
        let generation = self.doubao.generation;
        let shared = match self.shared_context_service() {
            Ok(shared) => shared,
            Err(error) => {
                self.doubao.status = DoubaoStatus::Disconnected(error.to_string().into());
                cx.notify();
                return false;
            }
        };
        let tool_adapter = crate::app::agent_runtime::start_doubao_adapter(
            project_id.clone().unwrap_or_default(),
            generation,
            self.database_paths.clone(),
            self.workspace_state.current_root.clone(),
            shared,
            window,
            cx,
        );
        let tools = Some(agent::NormaToolRuntime::new(tool_adapter));
        let database = self.database_paths.app_db_path().to_path_buf();
        let selected_voice = self.voice_settings.values().doubao_voice();
        let cancellation = CancellationToken::new();
        self.doubao.cancellation = Some(cancellation.clone());
        self.doubao.binding = Some(ChannelBinding {
            project: project_id.clone(),
            window: Some(window.window_handle().window_id().as_u64()),
        });
        self.doubao.status = DoubaoStatus::Connecting;
        tracing::info!(target: "norma_voice", generation, "Doubao report channel start requested");
        let (events_tx, events_rx) = async_channel::bounded(32);
        let (speech_tx, speech_rx) = async_channel::unbounded();
        self.doubao.speech = Some(speech_tx.clone());
        let (reports_tx, reports_rx) = async_channel::unbounded();
        let (recordings, listens) = if self.voice_settings.values().doubao_interaction_mode()
            == crate::settings::voice::VoiceInteractionMode::SingleVoiceCommand
        {
            let (sender, receiver) = async_channel::unbounded();
            self.doubao.recordings = Some(sender);
            (Some(receiver), None)
        } else {
            let (sender, receiver) = async_channel::unbounded();
            self.doubao.listens = Some(sender);
            (None, Some(receiver))
        };
        let input = input::SessionInput {
            events: speech_tx,
            speech: speech_rx,
            recordings,
            listens,
        };
        self.doubao.activation = Some(cx.observe_window_activation(window, |app, window, cx| {
            if !window.is_window_active() && app.doubao.recording.is_some() {
                // 失活时可能收不到松键事件；结束采集后仍须完成当前转写和回复。
                app.end_doubao_recording(cx);
            }
        }));
        self.doubao.reports = Some(reports_tx);
        let playback_events = self
            .voice_window_inset_events
            .sink(crate::app::voice_window_inset::TtsPlaybackSource::Doubao);
        let Ok(runtime) = crate::app::shared_tokio_runtime() else {
            self.doubao.stop();
            self.doubao.status =
                DoubaoStatus::Disconnected(LocalizedText::new("doubao.runtime_unavailable"));
            cx.notify();
            return false;
        };
        let context = self.start_doubao_context_requests(generation, project_id.clone(), cx);
        let previous_task = self.doubao.session_task.take();
        self.doubao.session_task = Some(runtime.spawn(async move {
            if let Some(previous_task) = previous_task {
                tracing::info!(target: "norma_voice", generation,
                    "waiting for previous Doubao session to finish");
                let _ = previous_task.await;
                tracing::info!(target: "norma_voice", generation,
                    "previous Doubao session finished");
            }
            if cancellation.is_cancelled() {
                return;
            }
            let result = run_session(
                (account, selected_voice),
                context,
                project_id,
                database,
                tools,
                cancellation,
                (events_tx.clone(), reports_rx, playback_events, input),
            )
            .await;
            tracing::info!(target: "norma_voice", generation,
                reason = result.as_ref().err().map(LocalizedText::as_str).unwrap_or("normal"),
                "Doubao session task finished");
            let _ = events_tx
                .send(DoubaoStatus::Disconnected(
                    result
                        .err()
                        .unwrap_or_else(|| LocalizedText::new("doubao.call_ended")),
                ))
                .await;
        }));
        // 实体级任务：窗口关闭、项目转入后台后通道状态仍需推进。
        cx.spawn(async move |this, cx| {
            while let Ok(status) = events_rx.recv().await {
                let keep = this
                    .update(cx, |app, cx| {
                        app.apply_doubao_session_status(generation, status, cx)
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
        true
    }

    fn apply_doubao_session_status(
        &mut self,
        generation: u64,
        status: DoubaoStatus,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.doubao.generation != generation {
            return false;
        }
        match &status {
            DoubaoStatus::UserTranscript(text) => self.record_assistant_user_message(text, cx),
            DoubaoStatus::AssistantReply(text) => self.record_assistant_norma_message(text, cx),
            _ => {}
        }
        let ended = match &status {
            DoubaoStatus::Disconnected(message) => {
                tracing::info!(target: "norma_voice", reason = %message, "voice conversation ended");
                Some(message.clone())
            }
            _ => None,
        };
        self.doubao.apply_status(status);
        if let Some(message) = ended {
            self.doubao.end_channel();
            self.doubao.note_channel_ended(&message, Instant::now());
            tracing::info!(target: "norma_voice", blocked = self.doubao.is_blocked(),
                "Doubao report channel ended");
        }
        cx.notify();
        self.doubao.is_active()
    }
}
