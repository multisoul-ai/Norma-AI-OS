use super::*;
use crate::app::dialogue::VoiceShortcutKeyDownAction;
use crate::app::voice::control::{
    is_voice_hold_modifier_release, is_voice_hold_release, matches_configured_voice_shortcut,
    VoiceTrigger,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DictationShortcutAction {
    Start,
    Stop,
}

pub(super) fn dictation_shortcut_action(
    trigger: crate::settings::voice::DictationTrigger,
    is_recording: bool,
) -> DictationShortcutAction {
    if trigger == crate::settings::voice::DictationTrigger::Toggle && is_recording {
        DictationShortcutAction::Stop
    } else {
        DictationShortcutAction::Start
    }
}

impl MultisoulApp {
    pub(crate) fn activate_ready_voice(
        &mut self,
        pointer_toggle: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.voice_settings.values().desktop_mode()
            == crate::settings::voice::DesktopVoiceMode::GptLive
        {
            self.activate_gpt_live_voice(pointer_toggle, window, cx);
            return;
        }
        if self.voice_settings.values().desktop_mode()
            == crate::settings::voice::DesktopVoiceMode::Doubao
        {
            if self.voice_settings.values().doubao_interaction_mode()
                == crate::settings::voice::VoiceInteractionMode::ContinuousListening
            {
                self.toggle_doubao_voice(window, cx);
            } else if pointer_toggle && self.doubao.recording.is_some() {
                self.end_doubao_recording(cx);
            } else {
                self.begin_doubao_recording(!pointer_toggle, window, cx);
            }
            return;
        }
        let continuous = self.voice_settings.values().interaction_mode()
            == crate::settings::voice::VoiceInteractionMode::ContinuousListening;
        match self.dialogue.voice_shortcut_key_down_action(continuous) {
            VoiceShortcutKeyDownAction::StartOneShotRecording => {
                if self.dialogue.is_active() {
                    self.stop_continuous_listening(cx);
                }
                let action = if pointer_toggle && self.is_voice_recording_shortcut() {
                    DictationShortcutAction::Stop
                } else {
                    dictation_shortcut_action(
                        self.voice_settings.values().dictation_trigger(),
                        self.is_voice_recording_shortcut(),
                    )
                };
                match action {
                    DictationShortcutAction::Start => {
                        self.begin_voice_recording(VoiceTrigger::Shortcut, window, cx)
                    }
                    DictationShortcutAction::Stop => {
                        self.end_voice_recording(VoiceTrigger::Shortcut, window, cx)
                    }
                }
            }
            VoiceShortcutKeyDownAction::StartContinuousListening => {
                self.start_continuous_listening(cx);
            }
            VoiceShortcutKeyDownAction::StopContinuousListening => {
                self.stop_continuous_listening(cx);
            }
            VoiceShortcutKeyDownAction::InterruptAssistantReply => {
                if self.dialogue.interrupt_assistant_reply_for_user_action() {
                    cx.notify();
                }
            }
            VoiceShortcutKeyDownAction::DismissError => {
                self.dismiss_voice_error();
                cx.notify();
            }
        }
    }

    pub(super) fn handle_configured_voice_key_down(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let voice_shortcut = self.shortcuts.voice_interaction_keystroke();
        if !matches_configured_voice_shortcut(keystroke, voice_shortcut.as_ref()) {
            return false;
        }
        if !matches!(
            self.voice_settings.readiness(),
            crate::settings::voice::VoiceEntryState::Ready
        ) {
            self.open_voice_settings(window, cx);
            cx.stop_propagation();
            return true;
        }
        self.activate_ready_voice(false, window, cx);
        cx.stop_propagation();
        true
    }

    pub(super) fn handle_configured_voice_hold_release(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let voice_shortcut = self.shortcuts.voice_interaction_keystroke();
        if self.voice_settings.values().desktop_mode()
            == crate::settings::voice::DesktopVoiceMode::GptLive
        {
            if self.gpt_live.is_shortcut_input()
                && is_voice_hold_release(keystroke, voice_shortcut.as_ref())
            {
                self.release_gpt_live_input(cx);
                return true;
            }
            return false;
        }
        if self.voice_settings.values().desktop_mode()
            == crate::settings::voice::DesktopVoiceMode::Doubao
        {
            if self.doubao.recording_from_shortcut
                && is_voice_hold_release(keystroke, voice_shortcut.as_ref())
            {
                self.end_doubao_recording(cx);
                return true;
            }
            return false;
        }
        if self.voice_settings.values().dictation_trigger()
            != crate::settings::voice::DictationTrigger::Hold
            || !self.is_voice_recording_shortcut()
            || !is_voice_hold_release(keystroke, voice_shortcut.as_ref())
        {
            return false;
        }
        self.end_voice_recording(VoiceTrigger::Shortcut, window, cx);
        true
    }

    pub(super) fn handle_configured_voice_modifier_release(
        &mut self,
        modifiers: &Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let voice_shortcut = self.shortcuts.voice_interaction_keystroke();
        if self.voice_settings.values().desktop_mode()
            == crate::settings::voice::DesktopVoiceMode::GptLive
        {
            if self.gpt_live.is_shortcut_input()
                && is_voice_hold_modifier_release(modifiers, voice_shortcut.as_ref())
            {
                self.release_gpt_live_input(cx);
            }
            return;
        }
        if self.voice_settings.values().desktop_mode()
            == crate::settings::voice::DesktopVoiceMode::Doubao
        {
            if self.doubao.recording_from_shortcut
                && is_voice_hold_modifier_release(modifiers, voice_shortcut.as_ref())
            {
                self.end_doubao_recording(cx);
            }
            return;
        }
        if self.voice_settings.values().dictation_trigger()
            == crate::settings::voice::DictationTrigger::Hold
            && self.is_voice_recording_shortcut()
            && is_voice_hold_modifier_release(modifiers, voice_shortcut.as_ref())
        {
            self.end_voice_recording(VoiceTrigger::Shortcut, window, cx);
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/app_keyboard_voice_tests.rs"]
mod voice_tests;

#[cfg(test)]
#[path = "../../../tests/unit/doubao_hold_keyboard_tests.rs"]
mod doubao_hold_tests;
