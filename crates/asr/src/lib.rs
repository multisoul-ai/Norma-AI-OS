mod catalog;
mod installation;
mod model_id;
mod model_root;
mod providers;
mod recognition;
mod support;

pub use catalog::{
    builtin_model_catalog, AsrArtifact, AsrCapabilities, AsrModelCatalog, AsrModelDescriptor,
    PARAFORMER_MODEL_ID, ZIPFORMER_BILINGUAL_ARCHIVE_URL, ZIPFORMER_BILINGUAL_MODEL_ID,
    ZIPFORMER_STREAMING_CTC_ZH_MODEL_ID, ZIPFORMER_STREAMING_ZH_MODEL_ID,
};
pub use installation::{
    install_model_archive, remove_installation, validate_installation, AsrInstallationError,
    AvailableAsrModel, InstallationManifest, INSTALLATION_MANIFEST_FILE,
};
pub use model_id::{AsrModelId, AsrModelIdError};
pub use model_root::{default_model_installation_root, model_installation_root};
pub use providers::*;
pub use recognition::*;
pub use support::{
    install_silero_vad_file, validate_silero_vad_installation, SILERO_VAD_ARCHIVE_BYTES,
    SILERO_VAD_ARCHIVE_URL, SILERO_VAD_DIRECTORY, SILERO_VAD_FILE, SILERO_VAD_SHA256,
};

#[cfg(test)]
#[path = "../tests/unit/model_id_tests.rs"]
mod model_id_tests;

#[cfg(test)]
#[path = "../tests/unit/catalog_tests.rs"]
mod catalog_tests;

#[cfg(test)]
#[path = "../tests/unit/installation_tests.rs"]
mod installation_tests;

#[cfg(test)]
#[path = "../tests/unit/installer_tests.rs"]
mod installer_tests;

#[cfg(test)]
#[path = "../tests/unit/support_tests.rs"]
mod support_tests;
