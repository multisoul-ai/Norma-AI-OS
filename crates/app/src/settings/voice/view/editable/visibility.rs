use super::{status, VoiceSettingsInputs, VoiceTextInput};
use crate::app::MultisoulApp;
use gpui::{App, Context};

impl VoiceSettingsInputs {
    pub(crate) fn api_key_revealed(&self, cx: &App) -> bool {
        self.api_key.read(cx).revealed
    }

    pub(crate) fn toggle_api_key_visibility(
        &self,
        secret: Option<&str>,
        cx: &mut Context<MultisoulApp>,
    ) {
        let secret = secret.map(ToOwned::to_owned);
        self.api_key.update(cx, |input, cx| {
            input.toggle_secret_preview(secret.as_deref(), cx)
        });
    }
}

impl VoiceTextInput {
    fn toggle_secret_preview(&mut self, secret: Option<&str>, cx: &mut Context<Self>) {
        if !self.masked {
            return;
        }
        if self.revealed {
            self.placeholder = status::CONFIGURED_API_KEY_PLACEHOLDER.into();
            self.sync_value(String::new(), cx);
            return;
        }
        let Some(secret) = secret.filter(|secret| !secret.is_empty()) else {
            return;
        };
        self.content = secret.to_string().into();
        self.original_content = "".into();
        self.selected_range = self.content.len()..self.content.len();
        self.selection_reversed = false;
        self.marked_range = None;
        self.revealed = true;
        self.last_layout = None;
        self.last_bounds = None;
        cx.notify();
    }
}
