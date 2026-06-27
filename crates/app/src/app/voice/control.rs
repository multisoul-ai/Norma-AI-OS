//! App-layer voice interaction state shared by keyboard and bottom bar entrypoints.

use gpui::{Keystroke, Modifiers, Task};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VoiceTrigger {
    Shortcut,
    BottomBar,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VoicePhase {
    Idle,
    Recording { trigger: VoiceTrigger },
    Transcribing,
    Planning { transcript: String },
    Confirming { transcript: String, reason: String },
    Executed { message: String },
    TranscriptOnly { transcript: String, reason: String },
    Error { message: String },
}

pub(crate) struct VoiceControlState {
    phase: VoicePhase,
    speech_session: Option<voice_sherpa::SpeechSession>,
    /// Separate receiver for partial transcription texts produced by the recognizer thread.
    /// Stored alongside the session so it can be moved into a GPUI async task in Task 2.
    pub(crate) partials_rx: Option<async_channel::Receiver<String>>,
    speech_task: Option<Task<()>>,
    pub(crate) partial_text: String,
}

impl Default for VoiceControlState {
    fn default() -> Self {
        Self {
            phase: VoicePhase::Idle,
            speech_session: None,
            partials_rx: None,
            speech_task: None,
            partial_text: String::new(),
        }
    }
}

impl VoiceControlState {
    pub(crate) fn phase(&self) -> &VoicePhase {
        &self.phase
    }

    pub(crate) fn begin_recording(&mut self, trigger: VoiceTrigger) -> bool {
        match self.phase {
            VoicePhase::Recording { .. }
            | VoicePhase::Transcribing
            | VoicePhase::Planning { .. } => false,
            VoicePhase::Idle
            | VoicePhase::Confirming { .. }
            | VoicePhase::Executed { .. }
            | VoicePhase::TranscriptOnly { .. }
            | VoicePhase::Error { .. } => {
                self.phase = VoicePhase::Recording { trigger };
                true
            }
        }
    }

    pub(crate) fn finish_recording(&mut self, trigger: VoiceTrigger) -> bool {
        if self.is_recording_from(trigger) {
            self.phase = VoicePhase::Transcribing;
            true
        } else {
            false
        }
    }

    pub(crate) fn is_recording_from(&self, trigger: VoiceTrigger) -> bool {
        matches!(self.phase, VoicePhase::Recording { trigger: active } if active == trigger)
    }

    pub(crate) fn set_planning(&mut self, transcript: impl Into<String>) {
        self.phase = VoicePhase::Planning {
            transcript: transcript.into(),
        };
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

    pub(crate) fn set_confirming(
        &mut self,
        transcript: impl Into<String>,
        reason: impl Into<String>,
    ) {
        self.phase = VoicePhase::Confirming {
            transcript: transcript.into(),
            reason: reason.into(),
        };
    }

    pub(crate) fn set_error(&mut self, message: impl Into<String>) {
        self.phase = VoicePhase::Error {
            message: message.into(),
        };
    }

    pub(crate) fn set_speech_session(
        &mut self,
        session: voice_sherpa::SpeechSession,
        partials_rx: async_channel::Receiver<String>,
    ) {
        self.speech_session = Some(session);
        self.partials_rx = Some(partials_rx);
    }

    pub(crate) fn set_speech_task(&mut self, task: Task<()>) {
        self.speech_task = Some(task);
    }

    pub(crate) fn clear_speech_task(&mut self) {
        self.speech_task = None;
    }

    pub(crate) fn cancel_speech_task(&mut self) {
        self.speech_task = None;
    }

    pub(crate) fn take_speech_session(&mut self) -> Option<voice_sherpa::SpeechSession> {
        self.speech_session.take()
    }

    pub(crate) fn cancel_speech_session(&mut self) {
        if let Some(session) = self.speech_session.take() {
            session.cancel();
        }
    }

    pub(crate) fn label(&self) -> String {
        match &self.phase {
            VoicePhase::Idle => "Hold Cmd+Shift+V".to_string(),
            VoicePhase::Recording { .. } => {
                if self.partial_text.is_empty() {
                    "Recording...".to_string()
                } else {
                    self.partial_text.clone()
                }
            }
            VoicePhase::Transcribing => "Transcribing...".to_string(),
            VoicePhase::Planning { .. } => "Planning...".to_string(),
            VoicePhase::Confirming { .. } => "Confirmation required".to_string(),
            VoicePhase::Executed { message } => message.clone(),
            VoicePhase::TranscriptOnly { transcript, .. } => transcript.clone(),
            VoicePhase::Error { message } => message.clone(),
        }
    }

    pub(crate) fn confirmation(&self) -> Option<(&str, &str)> {
        match &self.phase {
            VoicePhase::Confirming { transcript, reason } => Some((transcript, reason)),
            _ => None,
        }
    }
}

pub(crate) fn is_voice_hold_shortcut(keystroke: &Keystroke) -> bool {
    keystroke.key == "v"
        && keystroke.modifiers.platform
        && keystroke.modifiers.shift
        && !keystroke.modifiers.control
        && !keystroke.modifiers.alt
}

pub(crate) fn is_voice_hold_release(keystroke: &Keystroke) -> bool {
    keystroke.key == "v"
}

pub(crate) fn is_voice_hold_modifier_release(modifiers: &Modifiers) -> bool {
    !modifiers.platform || !modifiers.shift
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Keystroke;

    fn key(key: &str, platform: bool, shift: bool) -> Keystroke {
        Keystroke {
            key: key.into(),
            key_char: Some(key.into()),
            modifiers: Modifiers {
                platform,
                shift,
                ..Modifiers::default()
            },
        }
    }

    fn modifiers(platform: bool, shift: bool) -> Modifiers {
        Modifiers {
            platform,
            shift,
            ..Modifiers::default()
        }
    }

    /// VC-1: Cmd+Shift+V 是语音长按快捷键，普通 Cmd+V 不应匹配。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   voice key = key("v", platform=true, shift=true)
    ///   paste key = key("v", platform=true, shift=false)
    ///   other key = key("n", platform=true, shift=true)
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 检查 Cmd+Shift+V
    ///   2. 检查 Cmd+V
    ///   3. 检查 Cmd+Shift+N
    ///
    /// 预期结果：
    ///   - 正断言：Cmd+Shift+V 匹配语音长按
    ///   - 负断言：Cmd+V 和 Cmd+Shift+N 不匹配，避免抢占粘贴/新建
    #[test]
    fn test_voice_hold_shortcut_matches_cmd_shift_v_only() {
        let voice = key("v", true, true);
        let paste = key("v", true, false);
        let other = key("n", true, true);

        assert!(
            is_voice_hold_shortcut(&voice),
            "Cmd+Shift+V 应匹配语音长按快捷键"
        );
        assert!(
            !is_voice_hold_shortcut(&paste),
            "Cmd+V 是粘贴，不应触发语音"
        );
        assert!(
            !is_voice_hold_shortcut(&other),
            "Cmd+Shift+N 不是语音快捷键"
        );
    }

    /// VC-2: 重复 keydown 不重复启动录音，keyup 只结束对应 trigger。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   初始 phase = Idle
    ///   trigger = Shortcut
    ///   重复 begin 次数 = 2，第一次 true，第二次 false
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. begin_recording(Shortcut) → Recording
    ///   2. 再次 begin_recording(Shortcut) → 保持 Recording，返回 false
    ///   3. finish_recording(BottomBar) → trigger 不匹配，返回 false
    ///   4. finish_recording(Shortcut) → Transcribing
    ///
    /// 预期结果：
    ///   - 正断言：第一次开始成功，第二次幂等忽略
    ///   - 正断言：错误 trigger 不能结束录音
    ///   - 负断言：结束后不应仍处于 Recording
    #[test]
    fn test_voice_recording_state_is_idempotent_and_trigger_scoped() {
        let mut state = VoiceControlState::default();

        let first = state.begin_recording(VoiceTrigger::Shortcut);
        let duplicate = state.begin_recording(VoiceTrigger::Shortcut);
        let wrong_finish = state.finish_recording(VoiceTrigger::BottomBar);
        let right_finish = state.finish_recording(VoiceTrigger::Shortcut);

        assert!(first, "第一次 Shortcut keydown 应开始录音");
        assert!(!duplicate, "重复 keydown 不应重复启动录音");
        assert!(
            !wrong_finish,
            "BottomBar trigger 不应结束 Shortcut 启动的录音"
        );
        assert!(right_finish, "Shortcut keyup 应结束 Shortcut 录音");
        assert_ne!(
            state.phase(),
            &VoicePhase::Recording {
                trigger: VoiceTrigger::Shortcut,
            },
            "结束录音后不应仍处于 Recording"
        );
    }

    /// VC-3: Cmd 或 Shift 任一松开时，快捷键录音应进入释放路径。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   both_down = platform=true,  shift=true
    ///   cmd_up    = platform=false, shift=true
    ///   shift_up  = platform=true,  shift=false
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 检查 Cmd+Shift 仍按住 → 不触发释放
    ///   2. 检查 Cmd 已松开 → 触发释放
    ///   3. 检查 Shift 已松开 → 触发释放
    ///
    /// 预期结果：
    ///   - 负断言：两个 modifier 都还按住时不结束录音
    ///   - 正断言：Cmd 或 Shift 任一松开时结束录音，兜住缺失的 V keyup
    #[test]
    fn test_voice_hold_modifier_release_matches_cmd_or_shift_up() {
        let both_down = modifiers(true, true);
        let cmd_up = modifiers(false, true);
        let shift_up = modifiers(true, false);

        assert!(
            !is_voice_hold_modifier_release(&both_down),
            "Cmd+Shift 仍按住时不应结束快捷键录音"
        );
        assert!(
            is_voice_hold_modifier_release(&cmd_up),
            "Cmd 松开时应结束快捷键录音"
        );
        assert!(
            is_voice_hold_modifier_release(&shift_up),
            "Shift 松开时应结束快捷键录音"
        );
    }
}
