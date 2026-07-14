//! Voice entrypoint glue for the app shell.

mod agent_task;
pub(crate) mod bar;
pub(crate) mod control;
mod dictation;
mod execution;
mod history;
pub(crate) mod log;
pub(crate) mod modal;
mod planning;
pub(crate) mod playback;
mod refine;

use crate::app::voice::control::{VoicePhase, VoiceTrigger};
use crate::app::voice::log::{
    log_voice_confirmation_required, log_voice_executed, log_voice_executing,
    log_voice_planner_rejected, log_voice_runtime_failed, log_voice_transcript_only,
};
use crate::app::voice::planning::VoicePlanningOutcome;
use crate::app::{AppScreen, MultisoulApp};
use gpui::{ClickEvent, Context, Window};
use history::VoiceOutcome;
#[cfg(test)]
pub(crate) use planning::VoicePlanningJob;
pub(crate) use planning::VoicePlanningResult;
use voice::{CardRef, RiskLevel, VoiceCardKind, VoicePlanningRequest};
use voice_sherpa::SpeechError;

const DEFAULT_MAX_VOICE_STEPS: usize = 8;
const MICROPHONE_PRIVACY_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone";

pub(crate) fn microphone_privacy_settings_url() -> &'static str {
    MICROPHONE_PRIVACY_SETTINGS_URL
}

pub(crate) fn microphone_permission_recovery_url(error: &SpeechError) -> Option<&'static str> {
    matches!(error, SpeechError::MicrophoneDenied).then_some(microphone_privacy_settings_url())
}

impl MultisoulApp {
    pub(crate) fn begin_voice_recording(&mut self, trigger: VoiceTrigger, cx: &mut Context<Self>) {
        if !matches!(self.screen, AppScreen::Canvas) {
            self.voice.set_error("Open a project before using voice.");
            cx.notify();
            return;
        }
        self.cancel_agent_announcements();
        if !self.voice.begin_recording(trigger) {
            return;
        }
        if matches!(
            voice_transcript_source(),
            VoiceTranscriptSource::AppleSpeech
        ) {
            let values = self.voice_settings.values().clone();
            let model_root = self.voice_settings.model_root().to_path_buf();
            let input_device_id = voice_recording_input_device_id_from_value(
                self.voice_settings.values().input_device_id(),
            );
            // Two-step async to avoid CFRunLoop re-entrancy with GPUI's RefCell:
            //
            //   Step 1 (background): validate the selected model before requesting microphone
            //     permission, then request permission (which may block on the system dialog).
            //   Step 2 (main thread, via this.update): start_recording() — creates
            //     AVAudioEngine objects which are not Send. By this point the permission
            //     is already Authorized, so the call is non-blocking.
            let task = cx.spawn(async move |this, cx| {
                let provider = cx
                    .background_executor()
                    .spawn(async move {
                        let provider = crate::settings::voice::resolve_available_asr_model(
                            &values,
                            &model_root,
                        )
                        .map(voice_sherpa::SherpaProvider::new)?;
                        voice_sherpa::request_microphone_permission()?;
                        Ok(provider)
                    })
                    .await;
                let _ = this.update(cx, |app, cx| {
                    match provider {
                        Err(error) => {
                            app.voice.set_speech_error(&error);
                            app.save_voice_speech_error(error);
                        }
                        Ok(provider) => {
                            match provider
                                .start_recording_with_input_device(input_device_id.as_deref())
                            {
                                Ok((session, partials)) => {
                                    app.voice.set_speech_session(session);
                                    // Event-driven: wakes only when sherpa emits a partial,
                                    // exits automatically when the channel closes on session drop.
                                    let partials_task = cx.spawn(async move |this, cx| {
                                        while let Ok(text) = partials.recv().await {
                                            let _ = this.update(cx, |app, cx| {
                                                app.voice.partial_text = text;
                                                cx.notify();
                                            });
                                        }
                                    });
                                    // Store in the dedicated slot — NOT in speech_task — so that
                                    // the outer permission task (already in speech_task) and the
                                    // later finish task (set at key-up) never cancel the loop.
                                    app.voice.set_partials_task(partials_task);
                                }
                                Err(error) => {
                                    app.voice.set_speech_error(&error);
                                    app.save_voice_speech_error(error);
                                }
                            }
                        }
                    }
                    cx.notify();
                });
            });
            self.voice.set_speech_task(task);
        }
        cx.notify();
    }

    pub(crate) fn end_voice_recording(
        &mut self,
        trigger: VoiceTrigger,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.voice.finish_recording(trigger) {
            return;
        }
        cx.notify();
        self.process_finished_voice_recording(window, cx);
    }

    pub(crate) fn voice_label(&self) -> String {
        self.voice.label()
    }

    pub(crate) fn is_voice_recording_shortcut(&self) -> bool {
        matches!(
            self.voice.phase(),
            VoicePhase::Recording {
                trigger: VoiceTrigger::Shortcut,
            }
        )
    }

    pub(crate) fn on_voice_open_microphone_privacy_settings(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        if !self.can_open_microphone_privacy_settings() {
            return;
        }
        let url = microphone_privacy_settings_url().to_string();
        let _ = std::thread::Builder::new()
            .name("open_microphone_privacy_settings".to_string())
            .spawn(move || {
                if let Err(error) = std::process::Command::new("open").arg(url).spawn() {
                    tracing::warn!(%error, "failed to open macOS microphone privacy settings");
                }
            });
    }

    fn process_finished_voice_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match voice_transcript_source() {
            VoiceTranscriptSource::Environment(transcript) if !transcript.trim().is_empty() => {
                let transcript = transcript.trim().to_string();
                self.handle_final_voice_text(transcript, window, cx);
            }
            VoiceTranscriptSource::Environment(_) => {
                self.record_empty_voice_transcript();
            }
            VoiceTranscriptSource::AppleSpeech => {
                self.process_sherpa_speech_recording(window, cx);
            }
        }
        cx.notify();
    }

    fn process_sherpa_speech_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.voice.take_speech_session() else {
            let error = SpeechError::NotRecording;
            self.voice.set_speech_error(&error);
            self.save_voice_speech_error(error);
            return;
        };
        cx.notify();
        // Stop AVAudioEngine on the UI side, then move the blocking final join off-thread.
        let finalizer = match session.begin_finish() {
            Ok(finalizer) => finalizer,
            Err(error) => {
                self.voice.set_speech_error(&error);
                self.save_voice_speech_error(error);
                return;
            }
        };
        let finalize_task = cx
            .background_executor()
            .spawn(async move { finalizer.finish() });
        let task = cx.spawn_in(window, async move |this, cx| {
            let result = finalize_task.await;
            let _ = this.update_in(cx, |app, window, cx| {
                app.voice.clear_speech_task();
                app.voice.partial_text.clear();
                app.handle_sherpa_speech_result(result, window, cx);
            });
        });
        self.voice.set_speech_task(task);
    }

    fn handle_sherpa_speech_result(
        &mut self,
        result: Result<String, SpeechError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(transcript) if !transcript.trim().is_empty() => {
                let raw_transcript = transcript.trim().to_string();
                self.handle_final_voice_text(raw_transcript, window, cx);
            }
            Ok(_) | Err(SpeechError::EmptyTranscript) => {
                self.record_empty_voice_transcript();
            }
            Err(error) => {
                self.voice.set_speech_error(&error);
                self.save_voice_speech_error(error);
            }
        }
    }

    fn start_voice_planning(
        &mut self,
        raw_transcript: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        agent_task::start_direct_agent_task(self, raw_transcript, window, cx);
    }

    fn apply_voice_planning_result(
        &mut self,
        result: VoicePlanningResult,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let raw_transcript = result.raw_transcript;
        let refined_transcript = result.refined_transcript;
        let transcript = result.transcript;

        match result.outcome {
            VoicePlanningOutcome::ProviderError { error } => {
                self.voice
                    .set_error(format!("Voice planner failed: {error}"));
                self.save_voice_history(
                    raw_transcript,
                    refined_transcript,
                    "{}".into(),
                    0.0,
                    VoiceOutcome::new("low", "provider_error"),
                    Some(error),
                );
            }
            VoicePlanningOutcome::Rejected {
                planner_json,
                confidence,
                risk,
                error,
            } => {
                log_voice_planner_rejected(&transcript, &planner_json, &error);
                self.voice.set_transcript_only(
                    transcript.clone(),
                    format!("planner output rejected: {error}"),
                );
                self.save_voice_history(
                    raw_transcript,
                    refined_transcript,
                    planner_json,
                    confidence,
                    VoiceOutcome::new(risk_label(risk), "planner_rejected"),
                    Some(error),
                );
            }
            VoicePlanningOutcome::TranscriptOnly {
                planner_json,
                confidence,
                risk,
                reason,
            } => {
                let risk = risk_label(risk);
                log_voice_transcript_only(&transcript, &reason, confidence, risk);
                self.voice.set_transcript_only(transcript.clone(), reason);
                self.save_voice_history(
                    raw_transcript,
                    refined_transcript,
                    planner_json,
                    confidence,
                    VoiceOutcome::new(risk, "transcript_only"),
                    None,
                );
            }
            VoicePlanningOutcome::ConfirmationRequired {
                planner_json,
                confidence,
                risk,
                compiled,
            } => {
                let reason = compiled
                    .reason
                    .clone()
                    .unwrap_or_else(|| "Plan requires confirmation.".to_string());
                let risk = risk_label(risk);
                log_voice_confirmation_required(
                    &transcript,
                    compiled.commands.len(),
                    &reason,
                    confidence,
                    risk,
                );
                self.pending_voice_commands = compiled.commands;
                self.voice.set_confirming(transcript.clone(), reason);
                self.save_voice_history(
                    raw_transcript,
                    refined_transcript,
                    planner_json,
                    confidence,
                    VoiceOutcome::new(risk, "confirmation_required"),
                    None,
                );
            }
            VoicePlanningOutcome::Execute {
                planner_json,
                confidence,
                risk,
                compiled,
            } => {
                let risk = risk_label(risk);
                log_voice_executing(&transcript, compiled.commands.len(), confidence, risk);
                self.pending_voice_commands.clear();
                let result = self.execute_compiled_voice_plan(compiled.commands, window);
                match result {
                    Ok(message) => {
                        log_voice_executed(&transcript, &message);
                        self.voice.set_executed(message);
                        self.save_voice_history(
                            raw_transcript.clone(),
                            refined_transcript.clone(),
                            planner_json,
                            confidence,
                            VoiceOutcome::new(risk, "executed"),
                            None,
                        );
                    }
                    Err(error) => {
                        log_voice_runtime_failed(&transcript, &error);
                        self.voice
                            .set_error(format!("Voice command failed: {error}"));
                        self.save_voice_history(
                            raw_transcript.clone(),
                            refined_transcript.clone(),
                            planner_json,
                            confidence,
                            VoiceOutcome::new(risk, "runtime_error"),
                            Some(error),
                        );
                    }
                }
            }
        }
        cx.notify();
    }

    fn voice_planning_request(&self, transcript: String) -> VoicePlanningRequest {
        let selected_node_ids = self
            .runtime
            .document
            .nodes()
            .iter()
            .filter(|node| self.runtime.selection.contains(node.id))
            .map(|node| node.id)
            .collect();
        VoicePlanningRequest {
            transcript,
            cards: self
                .runtime
                .document
                .nodes()
                .iter()
                .map(|node| CardRef {
                    node_id: node.id,
                    title: node.kind.display_title(),
                    alias: match &node.kind {
                        canvas::CanvasNodeKind::Agent(spec) => spec.alias.clone(),
                        _ => None,
                    },
                    kind: voice_card_kind(&node.kind),
                    selected: self.runtime.selection.contains(node.id),
                    focused: matches!(
                        self.runtime.focus.mode(),
                        runtime::InputMode::NodeFocused(id) if id == node.id
                    ),
                })
                .collect(),
            focused_node_id: match self.runtime.focus.mode() {
                runtime::InputMode::NodeFocused(id) => Some(id),
                runtime::InputMode::CanvasSelect => None,
            },
            selected_node_ids,
            max_steps: DEFAULT_MAX_VOICE_STEPS,
        }
    }
}

fn risk_label(risk: RiskLevel) -> &'static str {
    match risk {
        RiskLevel::Low => "low",
        RiskLevel::Medium => "medium",
        RiskLevel::High => "high",
    }
}

fn voice_card_kind(kind: &canvas::CanvasNodeKind) -> VoiceCardKind {
    match kind {
        canvas::CanvasNodeKind::Term(_) => VoiceCardKind::Terminal,
        canvas::CanvasNodeKind::Agent(spec) => VoiceCardKind::Agent {
            profile: match spec.profile {
                canvas::AgentProfile::Claude => norma::protocol::AgentProfile::Claude,
                canvas::AgentProfile::Codex => norma::protocol::AgentProfile::Codex,
                canvas::AgentProfile::Cursor => norma::protocol::AgentProfile::Cursor,
                canvas::AgentProfile::OpenCode => norma::protocol::AgentProfile::OpenCode,
            },
        },
        canvas::CanvasNodeKind::Preview(_) => VoiceCardKind::Preview,
        canvas::CanvasNodeKind::Browser(_) => VoiceCardKind::Browser,
        canvas::CanvasNodeKind::Rect(_) => VoiceCardKind::Rect,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum VoiceTranscriptSource {
    Environment(String),
    AppleSpeech,
}

fn voice_transcript_source() -> VoiceTranscriptSource {
    voice_transcript_source_from_value(std::env::var("MULTISOUL_VOICE_TRANSCRIPT").ok())
}

fn voice_transcript_source_from_value(value: Option<String>) -> VoiceTranscriptSource {
    match value {
        Some(transcript) => VoiceTranscriptSource::Environment(transcript),
        None => VoiceTranscriptSource::AppleSpeech,
    }
}

fn voice_recording_input_device_id_from_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
#[path = "../../../tests/unit/app_voice_tests.rs"]
mod app_voice_tests;
