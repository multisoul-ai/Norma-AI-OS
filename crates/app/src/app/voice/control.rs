//! App-layer voice interaction state shared by keyboard and bottom bar entrypoints.

use gpui::{Keystroke, Modifiers, Task};

mod error;

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
    speech_session: Option<voice_sherpa::SpeechSession>,
    /// Holds the outer permission/finish task for the current voice recording.
    speech_task: Option<Task<()>>,
    /// Holds the partials-receive loop task separately so it is never overwritten
    /// by the permission or finish task, and continues running until the channel
    /// closes naturally after `session.finish()` drops the sender.
    partials_task: Option<Task<()>>,
    pub(crate) partial_text: String,
}

impl Default for VoiceControlState {
    fn default() -> Self {
        Self {
            phase: VoicePhase::Idle,
            microphone_settings_recovery_available: false,
            speech_session: None,
            speech_task: None,
            partials_task: None,
            partial_text: String::new(),
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
        self.microphone_settings_recovery_available = false;
        self.phase = VoicePhase::Error {
            message: message.into(),
        };
    }

    pub(crate) fn set_speech_session(&mut self, session: voice_sherpa::SpeechSession) {
        self.speech_session = Some(session);
    }

    pub(crate) fn set_speech_task(&mut self, task: Task<()>) {
        self.speech_task = Some(task);
    }

    pub(crate) fn set_partials_task(&mut self, task: Task<()>) {
        self.partials_task = Some(task);
    }

    pub(crate) fn clear_speech_task(&mut self) {
        self.speech_task = None;
    }
    pub(crate) fn cancel_speech_task(&mut self) {
        self.speech_task = None;
        self.partials_task = None;
    }
    pub(crate) fn take_speech_session(&mut self) -> Option<voice_sherpa::SpeechSession> {
        self.speech_session.take()
    }
    pub(crate) fn cancel_speech_session(&mut self) {
        if let Some(session) = self.speech_session.take() {
            session.cancel();
        }
        self.partials_task = None;
    }
    pub(crate) fn stop_for_model_switch(&mut self) {
        self.cancel_speech_task();
        self.cancel_speech_session();
        self.phase = VoicePhase::Idle;
        self.partial_text.clear();
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
            VoicePhase::Transcribing => "Transcribing...".to_string(),
            VoicePhase::Executed { message } => message.clone(),
            VoicePhase::TranscriptOnly { transcript, .. } => transcript.clone(),
            VoicePhase::Error { message } => message.clone(),
        }
    }
}

pub(crate) fn is_voice_hold_shortcut(keystroke: &Keystroke) -> bool {
    keystroke.key == "e"
        && keystroke.modifiers.platform
        && !keystroke.modifiers.control
        && !keystroke.modifiers.alt
        && !keystroke.modifiers.shift
}

pub(crate) fn is_voice_hold_release(keystroke: &Keystroke) -> bool {
    keystroke.key == "e"
}

pub(crate) fn is_voice_hold_modifier_release(modifiers: &Modifiers) -> bool {
    !modifiers.platform
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

    /// 辅助：构造一个处于指定 phase 的 `VoiceControlState`，用于 visual() 测试。
    fn state_with_phase(phase: VoicePhase) -> VoiceControlState {
        let mut state = VoiceControlState::default();
        match phase {
            VoicePhase::Idle => {}
            VoicePhase::Recording { trigger } => {
                state.begin_recording(trigger);
            }
            VoicePhase::Transcribing => {
                state.begin_recording(VoiceTrigger::Shortcut);
                state.finish_recording(VoiceTrigger::Shortcut);
            }
            VoicePhase::Executed { message } => state.set_executed(message),
            VoicePhase::TranscriptOnly { transcript, reason } => {
                state.set_transcript_only(transcript, reason)
            }
            VoicePhase::Error { message } => state.set_error(message),
        }
        state
    }

    /// VV-1: 每个 VoicePhase 全部映射到正确的 VoiceVisual 分类。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   Idle             → 期望 Idle
    ///   Recording(Shortcut) → 期望 Recording
    ///   Transcribing     → 期望 Processing（finish_recording 后进入）
    ///   Executed         → 期望 Idle（成功执行静默回 idle，不显示成功反馈）
    ///   TranscriptOnly   → 期望 Success（planner 拒绝/无命令，保留反馈）
    ///   Error            → 期望 Error
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 为每个 phase 构造 VoiceControlState
    ///   2. 调用 visual() 取视觉分类
    ///   3. 与期望分类逐一比对
    ///
    /// 预期结果：
    ///   - 正断言：每个 phase 的视觉分类与上表一致
    ///   - 负断言：Executed 不应显示 Success 反馈，而是静默回 Idle；
    ///     TranscriptOnly 不应被误判为 Processing。
    #[test]
    fn test_visual_maps_every_phase_correctly() {
        let cases = [
            (VoicePhase::Idle, VoiceVisual::Idle),
            (
                VoicePhase::Recording {
                    trigger: VoiceTrigger::Shortcut,
                },
                VoiceVisual::Recording,
            ),
            (VoicePhase::Transcribing, VoiceVisual::Processing),
            // 成功执行：静默回 Idle，不显示专门的"成功"反馈。
            (
                VoicePhase::Executed {
                    message: "done".into(),
                },
                VoiceVisual::Idle,
            ),
            (
                VoicePhase::TranscriptOnly {
                    transcript: "hello".into(),
                    reason: "no command".into(),
                },
                VoiceVisual::Success,
            ),
            (
                VoicePhase::Error {
                    message: "no mic".into(),
                },
                VoiceVisual::Error,
            ),
        ];

        for (phase, expected) in cases {
            let state = state_with_phase(phase.clone());
            let got = state.visual();
            assert_eq!(
                got, expected,
                "phase {:?} 应映射为 {:?}，实际得到 {:?}",
                phase, expected, got
            );
        }
    }

    /// VV-2: default() 状态（刚启动）的视觉分类必须是 Idle。
    ///
    /// 数据构造：
    ///   state = VoiceControlState::default()  // phase = Idle
    ///
    /// 执行过程：
    ///   1. 取默认状态
    ///   2. 调用 visual()
    ///
    /// 预期结果：
    ///   - 正断言：default 状态为 Idle，避免启动时误显示录音/错误外观
    #[test]
    fn test_visual_default_is_idle() {
        let state = VoiceControlState::default();
        assert_eq!(
            state.visual(),
            VoiceVisual::Idle,
            "默认状态应为 Idle，启动时不应误显示录音或错误外观"
        );
    }

    /// VC-1: Cmd+E 是语音长按快捷键，普通 Cmd+V 不应匹配。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   voice key = key("e", platform=true, shift=false)
    ///   paste key = key("v", platform=true, shift=false)
    ///   other key = key("n", platform=true, shift=false)
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 检查 Cmd+E
    ///   2. 检查 Cmd+V
    ///   3. 检查 Cmd+N
    ///
    /// 预期结果：
    ///   - 正断言：Cmd+E 匹配语音长按
    ///   - 负断言：Cmd+V 和 Cmd+N 不匹配，避免抢占粘贴/新建
    #[test]
    fn test_voice_hold_shortcut_matches_cmd_e_only() {
        let voice = key("e", true, false);
        let paste = key("v", true, false);
        let other = key("n", true, false);

        assert!(is_voice_hold_shortcut(&voice), "Cmd+E 应匹配语音长按快捷键");
        assert!(
            !is_voice_hold_shortcut(&paste),
            "Cmd+V 是粘贴，不应触发语音"
        );
        assert!(!is_voice_hold_shortcut(&other), "Cmd+N 不是语音快捷键");
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
    ///   3. finish_recording(Shortcut) → Transcribing
    ///
    /// 预期结果：
    ///   - 正断言：第一次开始成功，第二次幂等忽略
    ///   - 负断言：结束后不应仍处于 Recording
    #[test]
    fn test_voice_recording_state_is_idempotent_and_trigger_scoped() {
        let mut state = VoiceControlState::default();

        let first = state.begin_recording(VoiceTrigger::Shortcut);
        let duplicate = state.begin_recording(VoiceTrigger::Shortcut);
        let right_finish = state.finish_recording(VoiceTrigger::Shortcut);

        assert!(first, "第一次 Shortcut keydown 应开始录音");
        assert!(!duplicate, "重复 keydown 不应重复启动录音");
        assert!(right_finish, "Shortcut keyup 应结束 Shortcut 录音");
        assert_ne!(
            state.phase(),
            &VoicePhase::Recording {
                trigger: VoiceTrigger::Shortcut,
            },
            "结束录音后不应仍处于 Recording"
        );
    }

    /// VC-3: Cmd 松开时，快捷键录音应进入释放路径。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   cmd_down = platform=true,  shift=false
    ///   cmd_up   = platform=false, shift=false
    ///   shift_up = platform=true,  shift=true
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 检查 Cmd 仍按住 → 不触发释放
    ///   2. 检查 Cmd 已松开 → 触发释放
    ///   3. 检查 Shift 状态变化但 Cmd 仍按住 → 不触发释放
    ///
    /// 预期结果：
    ///   - 负断言：Cmd 仍按住时不结束录音
    ///   - 正断言：Cmd 松开时结束录音，兜住缺失的 E keyup
    #[test]
    fn test_voice_hold_modifier_release_matches_cmd_up_only() {
        let cmd_down = modifiers(true, false);
        let cmd_up = modifiers(false, false);
        let shift_changed = modifiers(true, true);

        assert!(
            !is_voice_hold_modifier_release(&cmd_down),
            "Cmd 仍按住时不应结束快捷键录音"
        );
        assert!(
            is_voice_hold_modifier_release(&cmd_up),
            "Cmd 松开时应结束快捷键录音"
        );
        assert!(
            !is_voice_hold_modifier_release(&shift_changed),
            "Shift 状态变化但 Cmd 仍按住时不应结束快捷键录音"
        );
    }
}
