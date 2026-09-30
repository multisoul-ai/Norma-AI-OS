use super::reply::{SelectedTtsModel, TtsClientBox};
use super::DialogueControlState;
use crate::app::MultisoulApp;
use gpui::Context;
use std::sync::{Arc, Mutex};

struct ResidentTtsClient {
    selection: Option<SelectedTtsModel>,
    client: TtsClientBox,
}

/// Owns the single process-resident TTS client shared by every speech path.
///
/// The mutex is held only by background synthesis work. Dialogue replies, Agent
/// announcements, and Settings previews therefore serialize access to the same
/// non-`Sync` Sherpa client instead of loading overlapping ONNX sessions.
#[derive(Clone, Default)]
pub(crate) struct SharedTtsRuntime {
    resident: Arc<Mutex<Option<ResidentTtsClient>>>,
}

impl SharedTtsRuntime {
    #[cfg(any(test, feature = "e2e-test-support"))]
    pub(crate) fn from_client_for_tests(
        selection: Option<SelectedTtsModel>,
        client: TtsClientBox,
    ) -> Self {
        Self {
            resident: Arc::new(Mutex::new(Some(ResidentTtsClient { selection, client }))),
        }
    }

    pub(crate) fn with_client<R, E>(
        &self,
        selection: Option<SelectedTtsModel>,
        create: impl FnOnce() -> Result<TtsClientBox, E>,
        operation: impl FnOnce(&mut dyn tts::TtsClient) -> Result<R, E>,
    ) -> Result<R, E> {
        let mut resident = self
            .resident
            .lock()
            .expect("shared TTS runtime lock should not be poisoned");
        let selection_matches = resident
            .as_ref()
            .is_some_and(|current| current.selection == selection);
        if !selection_matches {
            *resident = None;
        }
        if resident.is_none() {
            *resident = Some(ResidentTtsClient {
                selection,
                client: create()?,
            });
        }
        operation(
            resident
                .as_mut()
                .expect("resident TTS client was initialized")
                .client
                .as_mut(),
        )
    }

    pub(crate) fn invalidate(&mut self) -> Self {
        std::mem::take(self)
    }

    #[cfg(test)]
    pub(crate) fn resident_client_count_for_tests(&self) -> usize {
        usize::from(
            self.resident
                .lock()
                .expect("shared TTS test lock should not be poisoned")
                .is_some(),
        )
    }
}

impl DialogueControlState {
    pub(crate) fn selected_tts_model_matches(
        &self,
        model_id: Option<&tts::TtsModelId>,
        voice_id: Option<&tts::TtsVoiceId>,
    ) -> bool {
        let (Some(model_id), Some(voice_id)) = (model_id, voice_id) else {
            return false;
        };
        self.selected_tts_model.as_ref().is_some_and(|selected| {
            &selected.model_id == model_id && &selected.voice_id == voice_id
        })
    }

    pub(super) fn reply_tts_selection(&self) -> Option<SelectedTtsModel> {
        #[cfg(any(test, feature = "e2e-test-support"))]
        if let Some(selection) = self.reply_tts_selection_override.as_ref() {
            return Some(selection.clone());
        }
        self.selected_tts_model.clone()
    }
}

impl MultisoulApp {
    pub(crate) fn invalidate_tts_for_configuration_change(&mut self, cx: &mut Context<Self>) {
        let stale_runtime = self.dialogue.cancel_tts_for_configuration_change();
        cx.background_executor()
            .spawn(async move { drop(stale_runtime) })
            .detach();
    }
}
