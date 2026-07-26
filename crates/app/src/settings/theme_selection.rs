use crate::app::MultisoulApp;
use gpui::Context;
use storage::SettingsStore;
use ui::{Theme, ThemeId};

const THEME_SETTING_KEY: &str = "appearance.theme.v1";

pub(crate) struct ThemeSettingsState {
    storage: Option<SettingsStore>,
    pending_theme: Option<ThemeId>,
    persistence_running: bool,
}

impl ThemeSettingsState {
    pub(crate) fn load_from_store(store: SettingsStore) -> (ThemeId, Self) {
        Self::load_from(Some(store))
    }

    #[cfg(any(test, feature = "e2e-test-support"))]
    pub(crate) fn in_memory_default() -> (ThemeId, Self) {
        Self::load_from(None)
    }

    fn load_from(storage: Option<SettingsStore>) -> (ThemeId, Self) {
        let theme_id = storage
            .as_ref()
            .and_then(|store| match store.get_string(THEME_SETTING_KEY) {
                Ok(saved) => saved,
                Err(error) => {
                    tracing::warn!(error = %error, "failed to load theme setting");
                    None
                }
            })
            .and_then(|saved| {
                let theme_id = ThemeId::ALL
                    .into_iter()
                    .find(|theme_id| theme_id.as_str() == saved);
                if theme_id.is_none() {
                    tracing::warn!(saved, "ignored unknown saved theme id");
                }
                theme_id
            })
            .unwrap_or_default();
        (
            theme_id,
            Self {
                storage,
                pending_theme: None,
                persistence_running: false,
            },
        )
    }

    fn queue(&mut self, theme_id: ThemeId) {
        self.pending_theme = Some(theme_id);
    }

    fn take_pending_write(&mut self) -> Option<(SettingsStore, ThemeId)> {
        if self.persistence_running {
            return None;
        }
        let theme_id = self.pending_theme.take()?;
        let store = self.storage.clone()?;
        self.persistence_running = true;
        Some((store, theme_id))
    }

    fn finish_write(&mut self) {
        self.persistence_running = false;
    }
}

impl MultisoulApp {
    pub(crate) fn set_theme(&mut self, id: ThemeId, cx: &mut Context<Self>) {
        self.apply_theme(id, cx);
        self.theme_settings.queue(id);
        self.persist_theme_setting_in_background(cx);
    }

    fn apply_theme(&mut self, id: ThemeId, cx: &mut Context<Self>) {
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

    fn persist_theme_setting_in_background(&mut self, cx: &mut Context<Self>) {
        let Some((store, theme_id)) = self.theme_settings.take_pending_write() else {
            return;
        };
        let work = cx.background_executor().spawn(async move {
            store
                .set_string(THEME_SETTING_KEY, theme_id.as_str())
                .map_err(|error| error.to_string())
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                app.theme_settings.finish_write();
                if let Err(error) = result {
                    tracing::warn!(error, "failed to persist theme setting");
                }
                app.persist_theme_setting_in_background(cx);
            });
        })
        .detach();
    }
}
