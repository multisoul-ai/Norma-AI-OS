//! Engine-neutral Local TTS model and voice catalog.

mod catalog;
mod installation;
mod model_id;
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
