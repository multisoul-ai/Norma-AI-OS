use crate::settings::voice::jev::{
    self, JevConfigurationError, JevSettings, JevVerification, KeychainVault, SqliteVault,
};
use gpui::{AppContext, Context, Entity, Subscription, Window};
use ui::{
    component::input::{InputEvent, InputState},
    Theme,
};
mod view;

enum State {
    Loading,
    Unconfigured,
    Draft,
    Saved,
    Connected(JevVerification, bool),
    Failed(JevConfigurationError),
}

pub(in crate::settings::voice::view) struct JevPanel {
    pub(super) input: Entity<InputState>,
    pub(super) theme: Theme,
    settings: Option<JevSettings>,
    saved_key: Option<String>,
    environment_key: Option<String>,
    state: State,
    busy: bool,
    _subscription: Subscription,
}

impl JevPanel {
    pub(super) fn new(
        store: Option<storage::SettingsStore>,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder(ui::i18n::input_placeholder(
                    "voice.jev_key_placeholder",
                    window,
                    cx,
                ))
        });
        let subscription = cx.subscribe(&input, |view, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) && !view.busy {
                view.state = if !input.read(cx).value().trim().is_empty() {
                    State::Draft
                } else if view.saved_key.is_some() {
                    State::Saved
                } else {
                    State::Unconfigured
                };
                cx.notify();
            }
        });
        let settings = store.map(JevSettings::new);
        let load = settings.clone();
        let work = cx.background_executor().spawn(async move {
            let load = load.ok_or(JevConfigurationError::SettingsStorage)?;
            let vault = SqliteVault::new(load.store().clone());
            load.migrate_legacy_key(&vault, &KeychainVault);
            load.load_key(&vault)
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |view, cx| {
                view.busy = false;
                match result {
                    Ok(key) => {
                        view.saved_key = key;
                        view.state = if view.saved_key.is_some() {
                            State::Saved
                        } else {
                            State::Unconfigured
                        };
                    }
                    Err(error) => view.state = State::Failed(error),
                }
                cx.notify();
            });
        })
        .detach();
        Self {
            input,
            theme,
            settings,
            saved_key: None,
            environment_key: std::env::var("TYPESAFE_API_KEY")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            state: State::Loading,
            busy: true,
            _subscription: subscription,
        }
    }

    fn key(&self, cx: &Context<Self>) -> Option<String> {
        let value = self.input.read(cx).value();
        if !value.trim().is_empty() {
            Some(value.trim().to_string())
        } else {
            self.saved_key.clone()
        }
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(settings) = self.settings.clone() else {
            return;
        };
        let key = self.input.read(cx).value().trim().to_string();
        if key.is_empty() {
            return;
        }
        self.busy = true;
        let work = cx.background_executor().spawn(async move {
            let vault = SqliteVault::new(settings.store().clone());
            settings.save_key(&key, &vault)?;
            settings
                .load_key(&vault)?
                .ok_or(JevConfigurationError::SecureStorage)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.busy = false;
                match result {
                    Ok(key) => {
                        view.saved_key = Some(key);
                        view.state = State::Saved;
                        view.input
                            .update(cx, |input, cx| input.set_value("", window, cx));
                    }
                    Err(error) => view.state = State::Failed(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn verify(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(key) = self.key(cx) else {
            return;
        };
        let saved = self.saved_key.as_deref() == Some(key.as_str());
        self.busy = true;
        let work = cx
            .background_executor()
            .spawn(async move { jev::verify_key(key) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |view, cx| {
                view.busy = false;
                view.state = match result {
                    Ok(info) => {
                        tracing::info!(
                            elapsed_ms = info.elapsed_ms as u64,
                            "Jev configuration verification succeeded"
                        );
                        State::Connected(info, saved)
                    }
                    Err(error) => {
                        tracing::warn!(failure=?error,"Jev configuration verification failed");
                        State::Failed(error)
                    }
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(settings) = self.settings.clone() else {
            return;
        };
        self.busy = true;
        let work = cx.background_executor().spawn(async move {
            let vault = SqliteVault::new(settings.store().clone());
            settings.clear(&vault)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.busy = false;
                match result {
                    Ok(()) => {
                        view.saved_key = None;
                        view.state = State::Unconfigured;
                        view.input
                            .update(cx, |input, cx| input.set_value("", window, cx));
                    }
                    Err(error) => view.state = State::Failed(error),
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
