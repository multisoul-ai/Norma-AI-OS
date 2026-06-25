//! Voice entrypoint glue for the app shell.

pub(crate) mod control;
pub(crate) mod log;
pub(crate) mod modal;

use crate::app::voice::control::{VoicePhase, VoiceTrigger};
use crate::app::voice::log::{
    log_voice_confirmation_required, log_voice_executed, log_voice_executing,
    log_voice_planner_failed, log_voice_planner_rejected, log_voice_planning_started,
    log_voice_runtime_failed, log_voice_transcript_only,
};
use crate::app::{AppScreen, MultisoulApp};
use crate::norma::server::{execute_card_command_for_area, NormaCommandEffects, NormaExecution};
use gpui::{Context, MouseDownEvent, MouseUpEvent, Window};
use norma::protocol::{CardCommand, CardCommandResult};
use std::collections::HashMap;
use storage::VoiceHistoryEntry;
use voice::{
    compile_voice_plan, CardRef, ExecutableCommand, OllamaVoicePlanner, RiskLevel, VoiceCardKind,
    VoicePlanDecision, VoicePlanner, VoicePlanningRequest,
};
use voice_macos::{AppleSpeechProvider, SpeechError};

const DEFAULT_MAX_VOICE_STEPS: usize = 8;

struct VoiceHistoryDraft {
    transcript: String,
    planner_json: String,
    confidence: f64,
    risk: &'static str,
    status: &'static str,
    provider: Option<String>,
    model: Option<String>,
    error: Option<String>,
}

impl MultisoulApp {
    pub(crate) fn begin_voice_recording(&mut self, trigger: VoiceTrigger, cx: &mut Context<Self>) {
        if !matches!(self.screen, AppScreen::Canvas) {
            self.voice.set_error("Open a project before using voice.");
            cx.notify();
            return;
        }
        if !self.voice.begin_recording(trigger) {
            return;
        }
        if matches!(
            voice_transcript_source(),
            VoiceTranscriptSource::AppleSpeech
        ) {
            match AppleSpeechProvider::start_recording(AppleSpeechProvider::default_locale()) {
                Ok(session) => self.voice.set_speech_session(session),
                Err(error) => {
                    self.voice
                        .set_error(format!("Speech recognition failed: {error}"));
                    self.save_voice_speech_error(error);
                }
            }
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

    pub(crate) fn on_voice_bar_mouse_down(
        &mut self,
        _: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_voice_recording(VoiceTrigger::BottomBar, cx);
    }

    pub(crate) fn on_voice_bar_mouse_up(
        &mut self,
        _: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.end_voice_recording(VoiceTrigger::BottomBar, window, cx);
    }

    fn process_finished_voice_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match voice_transcript_source() {
            VoiceTranscriptSource::Environment(transcript) if !transcript.trim().is_empty() => {
                self.handle_voice_transcript(transcript.trim().to_string(), window, cx);
            }
            VoiceTranscriptSource::Environment(_) => {
                self.record_empty_voice_transcript();
            }
            VoiceTranscriptSource::AppleSpeech => {
                self.process_apple_speech_recording(window, cx);
            }
        }
        cx.notify();
    }

    fn process_apple_speech_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut session) = self.voice.take_speech_session() else {
            let error = SpeechError::NotRecording;
            self.voice
                .set_error(format!("Speech recognition failed: {error}"));
            self.save_voice_speech_error(error);
            return;
        };
        let task = cx.spawn_in(window, async move |this, cx| {
            let result = session.finish();
            let _ = this.update_in(cx, |app, window, cx| {
                app.voice.clear_speech_task();
                app.handle_apple_speech_result(result, window, cx);
            });
        });
        self.voice.set_speech_task(task);
    }

    fn handle_apple_speech_result(
        &mut self,
        result: Result<String, SpeechError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(transcript) if !transcript.trim().is_empty() => {
                self.handle_voice_transcript(transcript.trim().to_string(), window, cx);
            }
            Ok(_) | Err(SpeechError::EmptyTranscript) => {
                self.record_empty_voice_transcript();
            }
            Err(error) => {
                self.voice
                    .set_error(format!("Speech recognition failed: {error}"));
                self.save_voice_speech_error(error);
            }
        }
    }

    fn record_empty_voice_transcript(&mut self) {
        self.voice.set_transcript_only("", "empty transcript");
        self.save_voice_history("".into(), "{}".into(), 0.0, "low", "transcript_only", None);
    }

    fn handle_voice_transcript(
        &mut self,
        transcript: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice.set_planning(transcript.clone());
        let planner = OllamaVoicePlanner::from_env_or_default();

        let request = self.voice_planning_request(transcript.clone());
        log_voice_planning_started(&transcript, &planner, &request);
        let plan = match planner.plan(&request) {
            Ok(plan) => plan,
            Err(error) => {
                log_voice_planner_failed(&error, &transcript);
                self.voice
                    .set_error(format!("Voice planner failed: {error}"));
                self.save_voice_history(
                    transcript,
                    "{}".into(),
                    0.0,
                    "low",
                    "provider_error",
                    Some(error.to_string()),
                );
                cx.notify();
                return;
            }
        };
        let planner_json = serde_json::to_string(&plan).unwrap_or_else(|_| "{}".to_string());
        let confidence = f64::from(plan.confidence);
        let risk = risk_label(plan.risk);
        let decision = match compile_voice_plan(&request, plan) {
            Ok(decision) => decision,
            Err(error) => {
                log_voice_planner_rejected(&transcript, &planner_json, &error);
                self.voice.set_transcript_only(
                    transcript.clone(),
                    format!("planner output rejected: {error}"),
                );
                self.save_voice_history(
                    transcript,
                    planner_json,
                    confidence,
                    risk,
                    "planner_rejected",
                    Some(error.to_string()),
                );
                cx.notify();
                return;
            }
        };

        match decision {
            VoicePlanDecision::TranscriptOnly { reason } => {
                log_voice_transcript_only(&transcript, &reason, confidence, risk);
                self.voice.set_transcript_only(transcript.clone(), reason);
                self.save_voice_history(
                    transcript,
                    planner_json,
                    confidence,
                    risk,
                    "transcript_only",
                    None,
                );
            }
            VoicePlanDecision::Execute(compiled) if compiled.requires_confirmation => {
                let reason = compiled
                    .reason
                    .unwrap_or_else(|| "Plan requires confirmation.".to_string());
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
                    transcript,
                    planner_json,
                    confidence,
                    risk,
                    "confirmation_required",
                    None,
                );
            }
            VoicePlanDecision::Execute(compiled) => {
                log_voice_executing(&transcript, compiled.commands.len(), confidence, risk);
                self.pending_voice_commands.clear();
                let result = self.execute_compiled_voice_plan(compiled.commands, window);
                match result {
                    Ok(message) => {
                        log_voice_executed(&transcript, &message);
                        self.voice.set_executed(message);
                        self.save_voice_history(
                            transcript,
                            planner_json,
                            confidence,
                            risk,
                            "executed",
                            None,
                        );
                    }
                    Err(error) => {
                        log_voice_runtime_failed(&transcript, &error);
                        self.voice
                            .set_error(format!("Voice command failed: {error}"));
                        self.save_voice_history(
                            transcript,
                            planner_json,
                            confidence,
                            risk,
                            "runtime_error",
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

    pub(crate) fn execute_compiled_voice_plan(
        &mut self,
        commands: Vec<ExecutableCommand>,
        window: &mut Window,
    ) -> Result<String, String> {
        let area = self.visible_canvas_rect(window);
        let zoom = self.runtime.document.viewport.zoom;
        let mut created_by_step = HashMap::new();
        let mut last_message = "Voice command executed".to_string();

        for (step, command) in commands.into_iter().enumerate() {
            let card_command = match command {
                ExecutableCommand::Card(command) => command,
                ExecutableCommand::SendTextToPrevious {
                    previous_step,
                    text,
                    submit,
                } => {
                    let node_id = *created_by_step
                        .get(&previous_step)
                        .ok_or_else(|| format!("missing created node for step {previous_step}"))?;
                    CardCommand::SendText {
                        node_id,
                        text,
                        submit,
                    }
                }
                ExecutableCommand::FocusPrevious { previous_step } => {
                    let node_id = *created_by_step
                        .get(&previous_step)
                        .ok_or_else(|| format!("missing created node for step {previous_step}"))?;
                    CardCommand::Focus { node_id }
                }
            };
            let execution = execute_card_command_for_area(
                &mut self.runtime,
                &mut self.motion,
                &mut self.master_layout,
                card_command,
                area,
                zoom,
            )
            .map_err(|error| error.to_string())?;
            if let CardCommandResult::Created { node_id } = &execution.result {
                created_by_step.insert(step, *node_id);
                last_message = format!("Created card {node_id}");
            }
            self.apply_voice_execution_effects(execution);
        }

        Ok(last_message)
    }

    fn apply_voice_execution_effects(&mut self, execution: NormaExecution) {
        let NormaCommandEffects {
            canvas_dirty,
            session_dirty,
            notify: _,
        } = execution.effects;
        if canvas_dirty {
            self.mark_canvas_dirty();
        }
        if session_dirty {
            self.mark_session_dirty();
        }
    }

    pub(crate) fn save_voice_history(
        &mut self,
        transcript: String,
        planner_json: String,
        confidence: f64,
        risk: &'static str,
        status: &'static str,
        error: Option<String>,
    ) {
        self.save_voice_history_with_provider(VoiceHistoryDraft {
            transcript,
            planner_json,
            confidence,
            risk,
            status,
            provider: Some("ollama".to_string()),
            model: Some(OllamaVoicePlanner::configured_model()),
            error,
        });
    }

    fn save_voice_speech_error(&mut self, error: SpeechError) {
        self.save_voice_history_with_provider(VoiceHistoryDraft {
            transcript: "".into(),
            planner_json: "{}".into(),
            confidence: 0.0,
            risk: "low",
            status: "provider_error",
            provider: Some("apple_speech".to_string()),
            model: Some(AppleSpeechProvider::default_locale()),
            error: Some(error.to_string()),
        });
    }

    fn save_voice_history_with_provider(&mut self, draft: VoiceHistoryDraft) {
        let entry = VoiceHistoryEntry {
            transcript: draft.transcript,
            planner_json: draft.planner_json,
            confidence: draft.confidence,
            risk: draft.risk.to_string(),
            status: draft.status.to_string(),
            provider: draft.provider,
            model: draft.model,
            target_node_id: None,
            error: draft.error,
        };
        if let Err(error) = self.persistence.save_voice_history(&entry) {
            tracing::warn!(%error, "voice history save failed");
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
            },
        },
        canvas::CanvasNodeKind::Preview(_) => VoiceCardKind::Preview,
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

#[cfg(test)]
#[path = "../../../tests/unit/app_voice_tests.rs"]
mod app_voice_tests;
