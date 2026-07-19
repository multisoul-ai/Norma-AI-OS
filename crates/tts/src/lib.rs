//! Engine-neutral Local TTS model and voice catalog.

mod catalog;
mod installation;
mod model_id;
mod profile;
mod provider;
pub mod providers;
mod stream;
mod voice_catalog;
mod voice_reference;

pub use catalog::{
    builtin_model_catalog, TtsBackend, TtsCapabilities, TtsModelCatalog, TtsModelDescriptor,
    TtsModelPackage, TtsModelRecommendation, TtsReleaseGate, TtsSupportArtifact,
    TtsVoiceDescriptor, KOKORO_MULTI_LANG_MODEL_ID, MATCHA_ZH_EN_MODEL_ID, MATCHA_ZH_MODEL_ID,
    ZIPVOICE_DISTILL_INT8_MODEL_ID,
};
pub use installation::{
    default_model_installation_root, install_downloaded_package, model_installation_root,
    remove_installation, validate_installation, AvailableTtsModel, TtsInstallationError,
};
pub use model_id::{TtsModelId, TtsModelIdError, TtsVoiceId, TtsVoiceIdError};
pub use profile::{
    evaluate_remote_readiness, RemoteAccountStatus, RemoteTtsReadiness, RemoteTtsValidation,
    TtsProviderProfile, TtsValidatedSelection, VoiceSelectionProvenance,
};
pub use provider::{
    builtin_provider_registry, create_builtin_remote_tts_client, query_builtin_provider_voices,
    BuiltinRemoteTtsRequest, BuiltinTtsProviderRegistry, TtsProviderConfiguration,
    TtsProviderDescriptor, TtsProviderId, TtsProviderIdError, TtsProviderModelDescriptor,
    LOCAL_TTS_PROVIDER_ID, MINIMAX_SPEECH_HD_MODEL_ID, MINIMAX_SPEECH_TURBO_MODEL_ID,
    MINIMAX_TTS_PROVIDER_ID,
};
pub use stream::{TtsChunkSink, TtsClient, TtsError, TtsErrorKind, TtsStreamSummary};
pub use voice_catalog::{ProviderVoiceCatalog, ProviderVoiceDescriptor, TtsVoiceOrigin};
pub use voice_reference::{
    import_zipvoice_reference, load_zipvoice_reference, remove_zipvoice_reference,
    zipvoice_reference_root, ZipVoiceReference, ZipVoiceReferenceError,
};

#[cfg(test)]
#[path = "../tests/unit/catalog_tests.rs"]
mod catalog_tests;

#[cfg(test)]
#[path = "../tests/unit/voice_reference_tests.rs"]
mod voice_reference_tests;
