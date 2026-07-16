//! Voice entrypoint glue for the app shell.

pub(crate) mod bar;
pub(crate) mod control;
mod dictation;
mod execution;
mod history;
pub(crate) mod playback;

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
