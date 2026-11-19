use super::MultisoulApp;
use canvas::NodeId;
use gpui::Window;
use runtime::Runtime;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppScreen {
    Launchpad,
    DesktopWorkspace,
    Canvas,
}

pub(crate) fn mark_runtime_node_viewed_if_needed(runtime: &mut Runtime, node_id: NodeId) -> bool {
    runtime.mark_card_viewed(node_id)
}

pub(crate) fn window_visible_for_voice_inset(window: &Window) -> bool {
    #[cfg(any(test, feature = "e2e-test-support"))]
    {
        let _ = window;
        true
    }
    #[cfg(all(target_os = "macos", not(any(test, feature = "e2e-test-support"))))]
    {
        use objc2_app_kit::NSView;
        use raw_window_handle::RawWindowHandle;

        let Ok(handle) = raw_window_handle::HasWindowHandle::window_handle(window) else {
            return true;
        };
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return true;
        };
        let view = unsafe { &*handle.ns_view.as_ptr().cast::<NSView>() };
        view.window().is_none_or(|native_window| {
            native_window_allows_voice_inset(
                native_window.isVisible(),
                native_window.isMiniaturized(),
            )
        })
    }
    #[cfg(all(not(target_os = "macos"), not(any(test, feature = "e2e-test-support"))))]
    {
        true
    }
}

#[cfg(any(test, all(target_os = "macos", not(feature = "e2e-test-support"))))]
pub(crate) fn native_window_allows_voice_inset(visible: bool, minimized: bool) -> bool {
    visible && !minimized
}

impl MultisoulApp {
    /// 把设置里的光晕偏好、窗口可见性与聆听/思考相位同步到光晕总线。
    pub(crate) fn sync_voice_glow(&mut self, cx: &gpui::App) {
        use crate::effects::voice_glow::{VoiceGlowPhase, VoiceGlowStyle};
        let prefs = cx
            .try_global::<crate::settings::effects::state::EffectsSettingsState>()
            .map(|state| state.voice_glow)
            .unwrap_or_default();
        let bus = &self.voice_window_inset_events;
        bus.set_prefs(prefs);
        bus.set_enabled(
            prefs.style != VoiceGlowStyle::Off && self.voice_window_inset_window_visible,
        );
        let phase = match self.voice.phase() {
            crate::app::voice::control::VoicePhase::Recording { .. } => VoiceGlowPhase::Listening,
            crate::app::voice::control::VoicePhase::Transcribing => VoiceGlowPhase::Thinking,
            _ if self.doubao.listening.is_some() || self.doubao.recording.is_some() => {
                VoiceGlowPhase::Listening
            }
            _ if self.doubao.status == crate::app::doubao::DoubaoStatus::Thinking => {
                VoiceGlowPhase::Thinking
            }
            _ => VoiceGlowPhase::Idle,
        };
        bus.set_phase(phase);
    }
}

#[cfg(test)]
#[path = "../../tests/unit/app_state_tests.rs"]
mod tests;
