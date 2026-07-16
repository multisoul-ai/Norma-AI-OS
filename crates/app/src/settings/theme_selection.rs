use crate::app::MultisoulApp;
use gpui::Context;
use ui::{Theme, ThemeId};

impl MultisoulApp {
    pub(crate) fn set_theme(&mut self, id: ThemeId, cx: &mut Context<Self>) {
        self.theme_id = id;
        self.theme = Theme::for_id(id);
        let theme = self.theme;
        self.voice_settings_inputs.set_theme(theme, cx);
        if let Some(edit) = &self.browser_address_edit {
            edit.input
                .update(cx, |input, cx| input.set_theme(theme, cx));
        }
        cx.notify();
    }
}
