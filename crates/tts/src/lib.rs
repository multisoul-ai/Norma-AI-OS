//! Engine-neutral Local TTS model and voice catalog.

mod catalog;
mod installation;
mod model_id;

pub use catalog::{
    builtin_model_catalog, TtsBackend, TtsCapabilities, TtsModelCatalog, TtsModelDescriptor,
    TtsModelPackage, TtsModelRecommendation, TtsSupportArtifact, TtsVoiceDescriptor,
    KOKORO_MULTI_LANG_MODEL_ID, MATCHA_ZH_MODEL_ID,
};
pub use installation::{
    default_model_installation_root, install_downloaded_package, model_installation_root,
    remove_installation, validate_installation, AvailableTtsModel, TtsInstallationError,
};
pub use model_id::{TtsModelId, TtsModelIdError, TtsVoiceId, TtsVoiceIdError};

#[cfg(test)]
#[path = "../tests/unit/catalog_tests.rs"]
mod catalog_tests;
