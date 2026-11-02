use super::{SettingsStore, VoiceSettingsState};
use std::path::PathBuf;

#[derive(Clone, Default)]
pub(crate) struct A1SetupSnapshot {
    pub configured: bool,
}

impl VoiceSettingsState {
    pub(crate) fn credential_store(&self) -> Option<SettingsStore> {
        self.storage.clone()
    }

    /// StoredBootstrap calls this off the GPUI foreground, alongside other persisted settings.
    pub(super) fn initialize_capabilities(&mut self) {
        if cfg!(debug_assertions) && !cfg!(feature = "production-data") {
            if let Some(root) = std::env::var_os("NORMA_TEST_DATA_ROOT") {
                self.model_root = PathBuf::from(root).join("models");
            }
        }
        self.a1_setup = A1SetupSnapshot {
            configured: asr::A1Config::from_model_root(&self.model_root).is_ok(),
        };
    }

    pub(crate) fn a1_setup_snapshot(&self) -> A1SetupSnapshot {
        self.a1_setup.clone()
    }
}
