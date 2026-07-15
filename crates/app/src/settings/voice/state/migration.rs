//! One-way Voice Settings migrations applied when an existing store is loaded.

use super::values::LEGACY_KEY_DICTATION_TRIGGER;
use super::{VoiceInteractionMode, VoiceSettingsState};
use super::{KEY_INTERACTION_MODE, KEY_SINGLE_VOICE_COMMAND_TRIGGER};
use storage::SettingsStore;

impl VoiceSettingsState {
    pub(super) fn queue_legacy_single_voice_command_migration(&mut self) {
        let Some(store) = self.storage.clone() else {
            return;
        };
        let legacy_mode = legacy_mode_is_dictation(&store);
        let legacy_trigger = legacy_trigger_exists(&store);
        if legacy_mode {
            self.persist_string(
                KEY_INTERACTION_MODE,
                VoiceInteractionMode::SingleVoiceCommand.as_str(),
            );
        }
        if legacy_trigger {
            self.persist_string(
                KEY_SINGLE_VOICE_COMMAND_TRIGGER,
                self.values.dictation_trigger().as_str(),
            );
            self.delete_string(LEGACY_KEY_DICTATION_TRIGGER);
        }
    }
}

fn legacy_mode_is_dictation(store: &SettingsStore) -> bool {
    store
        .get_string(KEY_INTERACTION_MODE)
        .ok()
        .flatten()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("dictation"))
}

fn legacy_trigger_exists(store: &SettingsStore) -> bool {
    store
        .get_string(LEGACY_KEY_DICTATION_TRIGGER)
        .ok()
        .flatten()
        .is_some()
}
