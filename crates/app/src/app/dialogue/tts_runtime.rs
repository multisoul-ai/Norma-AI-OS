use super::reply::{SelectedTtsModel, TtsClientBox};
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

impl MultisoulApp {
    pub(crate) fn invalidate_tts_for_configuration_change(&mut self, cx: &mut Context<Self>) {
        let stale_runtime = self.dialogue.cancel_tts_for_configuration_change();
        cx.background_executor()
            .spawn(async move { drop(stale_runtime) })
            .detach();
    }
}
