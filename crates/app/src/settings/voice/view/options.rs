use super::{VoiceSettingsInputs, VoiceSettingsState, VoiceSetupDestination};
use gpui::ScrollHandle;
use ui::Theme;

pub(crate) struct VoiceSettingsView<'a> {
    pub(crate) live_call: crate::app::LiveCallView,
    pub(crate) theme: Theme,
    pub(crate) state: &'a VoiceSettingsState,
    pub(crate) inputs: &'a VoiceSettingsInputs,
    pub(crate) content_scroll: &'a ScrollHandle,
    pub(crate) setup_scroll_target: Option<VoiceSetupDestination>,
    pub(crate) voice_shortcut_label: &'a str,
}
