//! App-layer voice interaction state shared by keyboard and bottom bar entrypoints.

use gpui::{Keystroke, Modifiers, Task};

mod error;

pub(crate) type ActiveSpeechSession = super::recognition::AsrRecordingSession;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VoiceTrigger {
    Shortcut,
}

/// 视觉分类：把 `VoicePhase` 压缩成 5 类外观，供 `chrome.rs` 决定
/// 底部语音条的图标、颜色与布局分支。语义"这里应该长什么样"，与具体
/// `VoicePhase` 解耦，避免渲染层重复 match 8 个 arm。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VoiceVisual {
    /// 等待用户开始录音：弱化键帽提示。
    Idle,
    /// 正在收音：accent 色 + 实时转写。
    Recording,
    /// 录音结束后的处理流水线：转写 / 规划 / 待确认。
    Processing,
    /// 命令已执行或仅转写完成：成功反馈。
    Success,
    /// 录音或处理失败：错误反馈。
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VoicePhase {
    Idle,
    Recording { trigger: VoiceTrigger },
    Transcribing,
    Executed { message: String },
    TranscriptOnly { transcript: String, reason: String },
    Error { message: String },
}

pub(crate) struct VoiceControlState {
    phase: VoicePhase,
    microphone_settings_recovery_available: bool,
    speech_session: Option<ActiveSpeechSession>,
    /// Holds the outer permission/finish task for the current voice recording.
    speech_task: Option<Task<()>>,
    /// Holds the partials-receive loop task separately so it is never overwritten
    /// by the permission or finish task, and continues running until the channel
    /// closes naturally after `session.finish()` drops the sender.
    partials_task: Option<Task<()>>,
    duration_limit_task: Option<Task<()>>,
    pub(crate) partial_text: String,
    transcribing_message: Option<String>,
    last_duration_limit_feedback: Option<String>,
    #[cfg(feature = "e2e-test-support")]
    result_application_delay: Option<std::time::Duration>,
    next_recording_generation: u64,
    active_recording_generation: Option<u64>,
}

impl Default for VoiceControlState {
    fn default() -> Self {
        Self {
            phase: VoicePhase::Idle,
            microphone_settings_recovery_available: false,
            speech_session: None,
            speech_task: None,
            partials_task: None,
            duration_limit_task: None,
            partial_text: String::new(),
            transcribing_message: None,
            last_duration_limit_feedback: None,
            #[cfg(feature = "e2e-test-support")]
            result_application_delay: None,
            next_recording_generation: 0,
            active_recording_generation: None,
        }
    }
}

impl VoiceControlState {
    pub(crate) fn phase(&self) -> &VoicePhase {
        &self.phase
    }

    /// 把当前 `VoicePhase` 映射成渲染层需要的 `VoiceVisual` 分类。
    /// 任何 phase 变化都应能在这里找到唯一对应视觉，渲染层据此决定
    /// 图标颜色与布局，无需自己枚举 8 个 phase。
    ///
    /// 注意：`Executed`（命令成功执行）刻意映射为 `Idle` —— 成功就静默
    /// 回到底部条的 idle 外观，不显示专门的"成功"反馈（历史记录与日志已
    /// 独立保存）。`TranscriptOnly`（planner 拒绝/无可执行命令）不算"成功"，
    /// 仍保留 `Success` 反馈让用户知道发生了什么。
    pub(crate) fn visual(&self) -> VoiceVisual {
        match self.phase {
            VoicePhase::Idle | VoicePhase::Executed { .. } => VoiceVisual::Idle,
            VoicePhase::Recording { .. } => VoiceVisual::Recording,
            VoicePhase::Transcribing => VoiceVisual::Processing,
            VoicePhase::TranscriptOnly { .. } => VoiceVisual::Success,
            VoicePhase::Error { .. } => VoiceVisual::Error,
        }
    }

    pub(crate) fn begin_recording(&mut self, trigger: VoiceTrigger) -> bool {
        match self.phase {
            VoicePhase::Recording { .. } | VoicePhase::Transcribing => false,
            VoicePhase::Idle
            | VoicePhase::Executed { .. }
            | VoicePhase::TranscriptOnly { .. }
            | VoicePhase::Error { .. } => {
                self.next_recording_generation = self.next_recording_generation.wrapping_add(1);
                self.active_recording_generation = Some(self.next_recording_generation);
                self.last_duration_limit_feedback = None;
                self.phase = VoicePhase::Recording { trigger };
                true
            }
        }
    }

    pub(crate) fn finish_recording(&mut self, trigger: VoiceTrigger) -> bool {
        if self.is_recording_from(trigger) {
            self.active_recording_generation = None;
            self.phase = VoicePhase::Transcribing;
            self.transcribing_message = None;
            true
        } else {
            false
        }
    }

    pub(crate) fn finish_recording_at_duration_limit(&mut self) -> bool {
        if matches!(self.phase, VoicePhase::Recording { .. }) {
            self.active_recording_generation = None;
            self.phase = VoicePhase::Transcribing;
            let message = "Azure 60-second limit reached · Transcribing…".to_string();
            self.transcribing_message = Some(message.clone());
            self.last_duration_limit_feedback = Some(message);
            true
        } else {
            false
        }
    }

    pub(crate) fn is_recording_from(&self, trigger: VoiceTrigger) -> bool {
        matches!(self.phase, VoicePhase::Recording { trigger: active } if active == trigger)
    }

    pub(crate) fn set_executed(&mut self, message: impl Into<String>) {
        self.phase = VoicePhase::Executed {
            message: message.into(),
        };
    }

    pub(crate) fn set_transcript_only(
        &mut self,
        transcript: impl Into<String>,
        reason: impl Into<String>,
    ) {
        self.phase = VoicePhase::TranscriptOnly {
            transcript: transcript.into(),
            reason: reason.into(),
        };
    }

    pub(crate) fn set_error(&mut self, message: impl Into<String>) {
        self.active_recording_generation = None;
        self.microphone_settings_recovery_available = false;
        self.phase = VoicePhase::Error {
            message: message.into(),
        };
    }

    pub(crate) fn set_speech_session(&mut self, session: ActiveSpeechSession) {
        self.speech_session = Some(session);
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn has_speech_session(&self) -> bool {
        self.speech_session.is_some()
    }

    pub(crate) fn last_duration_limit_feedback(&self) -> Option<&str> {
        self.last_duration_limit_feedback.as_deref()
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn set_result_application_delay(&mut self, delay: Option<std::time::Duration>) {
        self.result_application_delay = delay;
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn result_application_delay(&self) -> Option<std::time::Duration> {
        self.result_application_delay
    }

    pub(crate) fn set_speech_task(&mut self, task: Task<()>) {
        self.speech_task = Some(task);
    }

    pub(crate) fn recording_generation(&self) -> u64 {
        self.next_recording_generation
    }

    pub(crate) fn accepts_recording_start(&self, generation: u64) -> bool {
        self.active_recording_generation == Some(generation)
            && matches!(self.phase, VoicePhase::Recording { .. })
    }

    pub(crate) fn accepts_recording_result(&self, generation: u64) -> bool {
        self.next_recording_generation == generation
            && matches!(self.phase, VoicePhase::Transcribing)
    }

    pub(crate) fn set_partials_task(&mut self, task: Task<()>) {
        self.partials_task = Some(task);
    }

    pub(crate) fn set_duration_limit_task(&mut self, task: Task<()>) {
        self.duration_limit_task = Some(task);
    }

    pub(crate) fn asr_duration_limit_reached(&self) -> bool {
        self.speech_session
            .as_ref()
            .is_some_and(ActiveSpeechSession::reached_duration_limit)
    }

    pub(crate) fn clear_speech_task(&mut self) {
        self.speech_task = None;
    }
    pub(crate) fn cancel_speech_task(&mut self) {
        self.speech_task = None;
        self.partials_task = None;
        self.duration_limit_task = None;
    }
    pub(crate) fn take_speech_session(&mut self) -> Option<ActiveSpeechSession> {
        self.speech_session.take()
    }
    pub(crate) fn cancel_speech_session(&mut self) {
        if let Some(session) = self.speech_session.take() {
            session.cancel();
        }
        self.partials_task = None;
        self.duration_limit_task = None;
    }
    pub(crate) fn stop_for_model_switch(&mut self) {
        self.next_recording_generation = self.next_recording_generation.wrapping_add(1);
        self.active_recording_generation = None;
        self.cancel_speech_task();
        self.cancel_speech_session();
        self.phase = VoicePhase::Idle;
        self.partial_text.clear();
        self.transcribing_message = None;
    }
    pub(crate) fn label(&self) -> String {
        match &self.phase {
            VoicePhase::Idle => "Voice".to_string(),
            VoicePhase::Recording { .. } => {
                if self.partial_text.is_empty() {
                    "Listening…".to_string()
                } else {
                    self.partial_text.clone()
                }
            }
            VoicePhase::Transcribing => self
                .transcribing_message
                .clone()
                .unwrap_or_else(|| "Transcribing...".to_string()),
            VoicePhase::Executed { message } => message.clone(),
            VoicePhase::TranscriptOnly { transcript, .. } => transcript.clone(),
            VoicePhase::Error { message } => message.clone(),
        }
    }
}

pub(crate) fn matches_configured_voice_shortcut(
    keystroke: &Keystroke,
    configured: Option<&Keystroke>,
) -> bool {
    configured.is_some_and(|expected| {
        keystroke.key == expected.key
            && keystroke.modifiers.platform == expected.modifiers.platform
            && keystroke.modifiers.control == expected.modifiers.control
            && keystroke.modifiers.alt == expected.modifiers.alt
            && keystroke.modifiers.shift == expected.modifiers.shift
            && keystroke.modifiers.function == expected.modifiers.function
    })
}

pub(crate) fn is_voice_hold_release(keystroke: &Keystroke, configured: Option<&Keystroke>) -> bool {
    configured.is_some_and(|expected| keystroke.key == expected.key)
}

pub(crate) fn is_voice_hold_modifier_release(
    modifiers: &Modifiers,
    configured: Option<&Keystroke>,
) -> bool {
    let Some(expected) = configured else {
        return false;
    };
    (expected.modifiers.platform && !modifiers.platform)
        || (expected.modifiers.control && !modifiers.control)
        || (expected.modifiers.alt && !modifiers.alt)
        || (expected.modifiers.shift && !modifiers.shift)
        || (expected.modifiers.function && !modifiers.function)
}

#[cfg(test)]
#[path = "../../../tests/unit/voice_recording_generation_tests.rs"]
mod recording_generation_tests;

#[cfg(test)]
#[path = "../../../tests/unit/voice_control_tests.rs"]
mod tests;
