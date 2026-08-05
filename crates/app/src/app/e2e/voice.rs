use crate::app::MultisoulApp;

/// 语音入口的稳定 e2e 投影：覆盖用户可见状态，不暴露录音或对话实现对象。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoiceE2eSnapshot {
    pub phase: &'static str,
    pub dialogue_phase: &'static str,
    pub visual: &'static str,
    pub label: String,
    pub microphone_recovery_available: bool,
    pub entry_state: &'static str,
    pub ready_notification_visible: bool,
    pub duration_limit_feedback: Option<String>,
    pub dock_label: String,
    pub dock_shortcut: String,
    pub caption_text: Option<String>,
    pub caption_preview: Option<String>,
    pub caption_expanded: bool,
    pub caption_visible: bool,
    pub caption_waveform_phase_milli: u16,
    pub caption_surface_opacity_milli: u16,
    pub recording_generation: u64,
}

impl MultisoulApp {
    pub fn voice_e2e_snapshot(&self) -> VoiceE2eSnapshot {
        VoiceE2eSnapshot {
            phase: match self.voice.phase() {
                crate::app::voice::control::VoicePhase::Idle => "idle",
                crate::app::voice::control::VoicePhase::Recording { .. } => "recording",
                crate::app::voice::control::VoicePhase::Transcribing => "transcribing",
                crate::app::voice::control::VoicePhase::Executed { .. } => "executed",
                crate::app::voice::control::VoicePhase::TranscriptOnly { .. } => "transcript_only",
                crate::app::voice::control::VoicePhase::Error { .. } => "error",
            },
            dialogue_phase: match self.dialogue.phase() {
                dialogue::DialoguePhase::Idle => "idle",
                dialogue::DialoguePhase::Listening => "listening",
                dialogue::DialoguePhase::Thinking => "thinking",
                dialogue::DialoguePhase::Speaking => "speaking",
            },
            visual: match self.effective_voice_visual() {
                crate::app::voice::control::VoiceVisual::Idle => "idle",
                crate::app::voice::control::VoiceVisual::Recording => "recording",
                crate::app::voice::control::VoiceVisual::Processing => "processing",
                crate::app::voice::control::VoiceVisual::Success => "success",
                crate::app::voice::control::VoiceVisual::Error => "error",
            },
            label: self.effective_voice_label(),
            microphone_recovery_available: self.can_open_microphone_privacy_settings(),
            entry_state: match self.voice_settings.readiness() {
                crate::settings::voice::VoiceEntryState::SetUp { .. } => "set_up",
                crate::settings::voice::VoiceEntryState::SettingUp { .. } => "setting_up",
                crate::settings::voice::VoiceEntryState::Ready => "ready",
                crate::settings::voice::VoiceEntryState::Unavailable { .. } => "unavailable",
            },
            ready_notification_visible: self.voice_ready_notification_visible,
            duration_limit_feedback: self
                .voice
                .last_duration_limit_feedback()
                .map(ToOwned::to_owned),
            dock_label: self.effective_voice_dock_label().to_string(),
            dock_shortcut: self.shortcuts.voice_interaction_label(),
            caption_text: self.voice_caption_text(),
            caption_preview: self.voice_caption_preview(),
            caption_expanded: self.voice.caption_expanded(),
            caption_visible: self.voice_caption_is_visible(),
            caption_waveform_phase_milli: self.voice_caption_waveform_phase_milli(),
            caption_surface_opacity_milli:
                crate::app::voice::caption::CAPTION_SURFACE_OPACITY_MILLI,
            recording_generation: self.voice.recording_generation(),
        }
    }

    pub fn show_voice_recording_transcript_for_e2e(&mut self, transcript: impl Into<String>) {
        self.voice.stop_for_model_switch();
        self.voice
            .begin_recording(crate::app::voice::control::VoiceTrigger::Shortcut);
        self.voice.partial_text = transcript.into();
    }

    pub fn show_voice_transcript_only_for_e2e(&mut self, transcript: impl Into<String>) {
        self.voice.stop_for_model_switch();
        self.voice
            .set_transcript_only(transcript, "e2e transcript-only fixture");
    }

    pub fn show_voice_thinking_for_e2e(&mut self, transcript: &str) {
        self.voice.stop_for_model_switch();
        self.voice.set_executed("Voice command sent");
        self.dialogue
            .begin_single_voice_command(transcript)
            .expect("E2E voice Thinking fixture must accept a non-empty transcript");
    }

    pub fn show_voice_replying_for_e2e(&mut self, transcript: &str) {
        self.show_voice_thinking_for_e2e(transcript);
        self.dialogue
            .session
            .on_reply_ready()
            .expect("E2E assistant reply fixture must transition Thinking to Speaking");
    }

    pub fn finish_voice_reply_for_e2e(&mut self) {
        self.dialogue
            .session
            .on_playback_finished_or_interrupted(false);
    }

    pub fn show_single_voice_turn_error_for_e2e(&mut self, message: impl Into<String>) {
        let _ = self
            .dialogue
            .begin_single_voice_command("e2e failed voice turn");
        self.voice.set_executed("Voice command sent");
        self.dialogue.recover_reply_failure(message);
    }

    pub fn show_continuous_voice_turn_error_for_e2e(&mut self, message: impl Into<String>) {
        self.dialogue.recover_reply_failure(message);
    }
}
