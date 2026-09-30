//! Voice entrypoint glue for the app shell.

pub(crate) mod bar;
pub(crate) mod caption;
pub(crate) mod control;
mod dictation;
mod execution;
mod history;
pub(crate) mod playback;
pub(crate) mod recognition;

use crate::app::voice::control::{VoicePhase, VoiceTrigger};
use crate::app::{AppScreen, MultisoulApp};
use gpui::{ClickEvent, Context, Window};
use voice_sherpa::SpeechError;

const MICROPHONE_PRIVACY_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone";

pub(crate) fn microphone_privacy_settings_url() -> &'static str {
    MICROPHONE_PRIVACY_SETTINGS_URL
}

pub(crate) fn microphone_permission_recovery_url(error: &SpeechError) -> Option<&'static str> {
    matches!(error, SpeechError::MicrophoneDenied).then_some(microphone_privacy_settings_url())
}

impl MultisoulApp {
    pub(crate) fn complete_voice_setup_if_ready(&mut self, cx: &mut Context<Self>) {
        if !self.voice_settings.reconcile_setup_completion() {
            return;
        }
        self.persist_voice_settings_in_background(cx);
        if self.settings_panel.open {
            return;
        }
        self.voice_ready_notification_visible = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(5))
                .await;
            let _ = this.update(cx, |app, cx| {
                app.voice_ready_notification_visible = false;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn begin_voice_recording(
        &mut self,
        trigger: VoiceTrigger,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.screen, AppScreen::Canvas) {
            self.voice.set_error("Open a project before using voice.");
            cx.notify();
            return;
        }
        self.cancel_agent_announcements();
        if !self.voice.begin_recording(trigger) {
            return;
        }
        self.dialogue.dismiss_error();
        let recording_generation = self.voice.recording_generation();
        if matches!(
            voice_transcript_source(),
            VoiceTranscriptSource::AppleSpeech
        ) {
            let values = self.voice_settings.values().clone();
            let model_root = self.voice_settings.model_root().to_path_buf();
            let store = self
                .voice_settings
                .credential_store()
                .expect("app voice settings need SQLite");
            let input_device_id = voice_recording_input_device_id_from_value(
                self.voice_settings.values().input_device_id(),
            );
            // Provider resolution, permission and microphone startup all remain off the GPUI
            // foreground. PcmStreamSession is Send because AVAudioEngine is owned by its capture
            // thread, so only the completed session is installed on the UI state.
            let task = cx.spawn_in(window, async move |this, cx| {
                let startup = cx
                    .background_executor()
                    .spawn(async move {
                        let factory = crate::app::dialogue::resolve_asr_recognizer_factory(
                            &values,
                            &model_root,
                            &store,
                        )
                        .map_err(|error| SpeechError::Recognizer(error.to_string()))?;
                        voice_input::request_microphone_permission().map_err(SpeechError::from)?;
                        recognition::AsrRecordingSession::start(input_device_id, factory.as_ref())
                            .map_err(SpeechError::Recognizer)
                    })
                    .await;
                let _ = this.update_in(cx, |app, window, cx| {
                    if !app.voice.accepts_recording_start(recording_generation) {
                        if let Ok((session, _partials)) = startup {
                            session.cancel();
                        }
                        return;
                    }
                    match startup {
                        Err(error) => {
                            app.voice.set_speech_error(&error);
                            app.save_voice_speech_error(error);
                        }
                        Ok((session, partials)) => {
                            app.voice.set_speech_session(session);
                            app.start_asr_duration_limit_monitor(window, cx);
                            // Event-driven: wakes only when the shared pipeline emits a partial,
                            // exits automatically when the channel closes on session drop.
                            let partials_task = cx.spawn(async move |this, cx| {
                                while let Ok(text) = partials.recv().await {
                                    let _ = this.update(cx, |app, cx| {
                                        app.voice.partial_text = text;
                                        cx.notify();
                                    });
                                }
                            });
                            // Store separately so permission/start and finish tasks cannot cancel
                            // the partial event pump by replacing the main speech task.
                            app.voice.set_partials_task(partials_task);
                        }
                    }
                    cx.notify();
                });
            });
            self.voice.set_speech_task(task);
        }
        cx.notify();
    }

    fn start_asr_duration_limit_monitor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let task = cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(100))
                .await;
            let reached = this
                .update(cx, |app, _cx| app.voice.asr_duration_limit_reached())
                .unwrap_or(false);
            if reached {
                let _ = this.update_in(cx, |app, window, cx| {
                    if app.voice.finish_recording_at_duration_limit() {
                        app.process_finished_voice_recording(window, cx);
                        cx.notify();
                    }
                });
                break;
            }
            let still_recording = this
                .update(cx, |app, _cx| {
                    matches!(app.voice.phase(), VoicePhase::Recording { .. })
                })
                .unwrap_or(false);
            if !still_recording {
                break;
            }
        });
        self.voice.set_duration_limit_task(task);
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

    pub(crate) fn dismiss_voice_error(&mut self) {
        self.voice.dismiss_error();
        self.dialogue.dismiss_error();
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
                self.process_active_speech_recording(window, cx);
            }
        }
        cx.notify();
    }

    fn process_active_speech_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.voice.take_speech_session() else {
            let error = SpeechError::NotRecording;
            self.voice.set_speech_error(&error);
            self.save_voice_speech_error(error);
            return;
        };
        let recording_generation = self.voice.recording_generation();
        #[cfg(feature = "e2e-test-support")]
        let result_application_delay = self.voice.result_application_delay();
        cx.notify();
        let finalize_task = cx
            .background_executor()
            .spawn(async move { session.finish() });
        let task = cx.spawn_in(window, async move |this, cx| {
            let result = finalize_task.await;
            #[cfg(feature = "e2e-test-support")]
            if let Some(delay) = result_application_delay {
                cx.background_executor().timer(delay).await;
            }
            let _ = this.update_in(cx, |app, window, cx| {
                if !app.voice.accepts_recording_result(recording_generation) {
                    return;
                }
                app.voice.clear_speech_task();
                app.voice.partial_text.clear();
                match result {
                    Ok(transcript) if !transcript.trim().is_empty() => {
                        app.handle_final_voice_text(transcript.trim().to_string(), window, cx)
                    }
                    Ok(_) => app.record_empty_voice_transcript(),
                    Err(error) => app
                        .voice
                        .set_error(format!("Speech recognition failed: {error}")),
                }
            });
        });
        self.voice.set_speech_task(task);
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
