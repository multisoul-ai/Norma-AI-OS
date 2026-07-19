use crate::app::MultisoulApp;

/// 语音入口的稳定 e2e 投影：覆盖用户可见状态，不暴露录音或对话实现对象。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoiceE2eSnapshot {
    pub phase: &'static str,
    pub dialogue_phase: &'static str,
    pub label: String,
    pub microphone_recovery_available: bool,
    pub entry_state: &'static str,
    pub ready_notification_visible: bool,
    pub duration_limit_feedback: Option<String>,
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
        }
    }
}
