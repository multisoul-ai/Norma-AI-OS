mod credentials;
mod download_progress;
mod models;
mod readiness;
mod state;
pub(crate) mod tts_models;
mod view;

#[cfg(feature = "e2e-test-support")]
pub(crate) use models::set_e2e_fake_asr_install;
pub(crate) use models::{AsrModelInstallState, AsrModelManagerState};
pub(crate) use readiness::{
    evaluate_voice_readiness, VoiceEntryState, VoiceReadinessInputs, VoiceRequirement,
    VoiceRequirementState,
};
#[cfg(test)]
pub(crate) use state::resolve_available_asr_model;
pub(crate) use state::ProviderDiscoveryStatus;
pub(crate) use state::{
    resolve_builtin_asr_profile, DesktopVoiceMode, DictationTrigger, PendingVoiceSettingWrite,
    TtsActivationRollback, VoiceInteractionMode, VoiceSettingsField, VoiceSettingsState,
    VoiceSettingsValues, VoiceSetupDestination,
};
pub(crate) use tts_models::{TtsModelInstallState, TtsModelManagerState};
pub(crate) use view::{VoiceSettingsInputs, VoiceSettingsView};

pub(crate) use state::{
    provider_configuration_differs, tts_configuration_differs, SavedVoiceSettings,
};
#[cfg(test)]
pub(crate) use state::{AgentProviderKind, EnvVoiceSettings};
pub(crate) use view::voice_settings;
