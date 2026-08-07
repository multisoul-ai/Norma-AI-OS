use super::VoiceSettingsState;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct ZipVoiceReferenceSettings {
    draft_path: String,
    draft_transcript: String,
    ready: bool,
    importing: bool,
    removing: bool,
    operation_generation: u64,
    input_generation: u64,
}

impl VoiceSettingsState {
    pub(crate) fn zipvoice_reference_ready(&self) -> bool {
        self.zipvoice_reference.ready
    }

    pub(crate) fn zipvoice_reference_importing(&self) -> bool {
        self.zipvoice_reference.importing
    }

    pub(crate) fn zipvoice_reference_operation_busy(&self) -> bool {
        self.zipvoice_reference.importing || self.zipvoice_reference.removing
    }

    pub(crate) fn zipvoice_reference_can_import(&self) -> bool {
        !self.zipvoice_reference_operation_busy()
            && !self.zipvoice_reference.draft_path.trim().is_empty()
            && !self.zipvoice_reference.draft_transcript.trim().is_empty()
    }

    pub(crate) fn zipvoice_reference_draft_path(&self) -> &str {
        &self.zipvoice_reference.draft_path
    }

    pub(crate) fn zipvoice_reference_draft_transcript(&self) -> &str {
        &self.zipvoice_reference.draft_transcript
    }

    pub(crate) fn zipvoice_reference_input_generation(&self) -> u64 {
        self.zipvoice_reference.input_generation
    }

    pub(crate) fn set_zipvoice_reference_draft_path(&mut self, path: String, reset_input: bool) {
        self.zipvoice_reference.draft_path = path;
        if reset_input {
            self.zipvoice_reference.input_generation =
                self.zipvoice_reference.input_generation.wrapping_add(1);
        }
    }

    pub(crate) fn set_zipvoice_reference_draft_transcript(&mut self, transcript: String) {
        self.zipvoice_reference.draft_transcript = transcript;
    }

    pub(crate) fn reset_zipvoice_reference_inputs_for_submit(&mut self) {
        self.zipvoice_reference.input_generation =
            self.zipvoice_reference.input_generation.wrapping_add(1);
    }

    pub(crate) fn begin_zipvoice_reference_import(&mut self) -> Option<(u64, String, String)> {
        if !self.zipvoice_reference_can_import() {
            return None;
        }
        self.zipvoice_reference.operation_generation =
            self.zipvoice_reference.operation_generation.wrapping_add(1);
        self.zipvoice_reference.importing = true;
        self.status = None;
        Some((
            self.zipvoice_reference.operation_generation,
            self.zipvoice_reference.draft_path.clone(),
            self.zipvoice_reference.draft_transcript.clone(),
        ))
    }

    pub(crate) fn finish_zipvoice_reference_import(
        &mut self,
        generation: u64,
        result: Result<tts::ZipVoiceReference, String>,
    ) -> bool {
        if generation != self.zipvoice_reference.operation_generation
            || !self.zipvoice_reference.importing
        {
            return false;
        }
        self.zipvoice_reference.importing = false;
        match result {
            Ok(reference) => {
                self.zipvoice_reference.ready = true;
                self.zipvoice_reference.draft_path =
                    reference.audio_path().to_string_lossy().into_owned();
                self.zipvoice_reference.draft_transcript = reference.transcript().to_string();
                self.zipvoice_reference.input_generation =
                    self.zipvoice_reference.input_generation.wrapping_add(1);
                self.report_tts_status("ZipVoice reference voice imported.", false);
                true
            }
            Err(error) => {
                self.report_tts_status(
                    format!("Could not import ZipVoice reference voice: {error}"),
                    true,
                );
                false
            }
        }
    }

    pub(crate) fn begin_zipvoice_reference_remove(&mut self) -> Option<u64> {
        if self.zipvoice_reference_operation_busy() {
            return None;
        }
        self.zipvoice_reference.operation_generation =
            self.zipvoice_reference.operation_generation.wrapping_add(1);
        self.zipvoice_reference.removing = true;
        self.status = None;
        Some(self.zipvoice_reference.operation_generation)
    }

    pub(crate) fn apply_zipvoice_reference_scan(
        &mut self,
        result: Result<tts::ZipVoiceReference, tts::ZipVoiceReferenceError>,
    ) {
        if self.zipvoice_reference_operation_busy() {
            return;
        }
        match result {
            Ok(reference) => {
                self.zipvoice_reference.ready = true;
                if self.zipvoice_reference.draft_path.is_empty() {
                    self.zipvoice_reference.draft_path =
                        reference.audio_path().to_string_lossy().into_owned();
                }
                if self.zipvoice_reference.draft_transcript.is_empty() {
                    self.zipvoice_reference.draft_transcript = reference.transcript().to_string();
                }
            }
            Err(_) => self.zipvoice_reference.ready = false,
        }
    }

    pub(crate) fn finish_zipvoice_reference_remove(
        &mut self,
        generation: u64,
        result: Result<(), String>,
    ) -> bool {
        if generation != self.zipvoice_reference.operation_generation
            || !self.zipvoice_reference.removing
        {
            return false;
        }
        self.zipvoice_reference.removing = false;
        match result {
            Ok(()) => {
                let operation_generation = self.zipvoice_reference.operation_generation;
                self.zipvoice_reference = ZipVoiceReferenceSettings::default();
                self.zipvoice_reference.operation_generation = operation_generation;
                self.report_tts_status("ZipVoice reference voice removed.", false);
                true
            }
            Err(error) => {
                self.report_tts_status(
                    format!("Could not remove ZipVoice reference voice: {error}"),
                    true,
                );
                false
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/zipvoice_reference_state_tests.rs"]
mod zipvoice_reference_state_tests;
