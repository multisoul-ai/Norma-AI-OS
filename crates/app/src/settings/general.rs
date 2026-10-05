//! App-wide interface language. SQLite work is confined to the background writer.
use gpui::{div, prelude::*, px, rgba, App, Context, Div, Global};
use storage::SettingsStore;
use ui::i18n::{text, LanguagePreference, Locale, LocalizedText};
use ui::{component::radio::Radio, Theme};

mod persistence;
use persistence::LanguagePersistence;

pub(super) const LANGUAGE_KEY: &str = "general.language.v1";

pub(crate) struct PreparedLanguageSettings {
    preference: LanguagePreference,
    locale: Locale,
    store: Option<SettingsStore>,
}

impl PreparedLanguageSettings {
    pub(crate) fn load_from_store(store: SettingsStore, languages: &[String]) -> Self {
        let saved = match store.get_string(LANGUAGE_KEY) {
            Ok(saved) => saved,
            Err(error) => {
                tracing::warn!(%error, "failed to load interface language");
                None
            }
        };
        let preference = LanguagePreference::from_saved(saved.as_deref());
        Self {
            preference,
            locale: preference.resolve(languages),
            store: Some(store),
        }
    }
}

pub(crate) struct AppLanguage {
    preference: LanguagePreference,
    locale: Locale,
    writer: Option<LanguagePersistence>,
    save_error: Option<LocalizedText>,
    latest_write: Option<u64>,
    quit_pending: bool,
}
impl Global for AppLanguage {}

pub(crate) fn is_ready(cx: &App) -> bool {
    cx.has_global::<AppLanguage>()
}

pub(crate) fn open_initial_window(cx: &mut App) {
    let load = cx.background_executor().spawn(async {
        let languages = system_languages();
        match crate::database::default_database_context() {
            Ok((_, store)) => {
                PreparedLanguageSettings::load_from_store(store.settings_store(), &languages)
            }
            Err(error) => {
                tracing::warn!(%error, "interface language storage unavailable");
                PreparedLanguageSettings {
                    preference: LanguagePreference::System,
                    locale: LanguagePreference::System.resolve(&languages),
                    store: None,
                }
            }
        }
    });
    cx.spawn(async move |cx| {
        let prepared = load.await;
        cx.update(|cx| {
            install(prepared, cx);
            if let Err(error) = crate::license_gate::open_gated_window(cx) {
                tracing::error!(%error, "failed to open initial Norma window");
            }
            cx.activate(true);
        })
        .ok();
    })
    .detach();
}

fn install(prepared: PreparedLanguageSettings, cx: &mut App) {
    if is_ready(cx) {
        return;
    }
    let writer = prepared.store.map(LanguagePersistence::start);
    let events = writer.as_ref().map(|writer| writer.results.clone());
    cx.set_global(AppLanguage {
        preference: prepared.preference,
        locale: prepared.locale,
        writer,
        save_error: None,
        latest_write: None,
        quit_pending: false,
    });
    ui::i18n::set_app_locale(prepared.locale, cx);
    cx.set_menus(crate::menu_bar::menu_bar());
    if let Some(events) = events {
        cx.spawn(async move |cx| {
            while let Ok(result) = events.recv().await {
                if cx
                    .update(|cx| {
                        cx.update_global::<AppLanguage, _>(|language, _| {
                            if language.preference == result.preference
                                && language.latest_write == Some(result.sequence)
                            {
                                language.save_error = result.result.err().map(|reason| {
                                    LocalizedText::new("general.save_failed")
                                        .with_arg("path", result.path.display())
                                        .with_arg("reason", reason)
                                });
                            }
                        });
                        cx.refresh_windows();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }
    cx.on_app_quit(|cx| {
        let writer = cx
            .try_global::<AppLanguage>()
            .and_then(|language| language.writer.clone());
        async move {
            if let Some(writer) = writer {
                writer.shutdown().await;
            }
        }
    })
    .detach();
}

/// GPUI's final shutdown observer has a 100ms budget. Await the serial writer
/// while the foreground event loop is still alive, including choices made
/// during the wait. A cancelled quit must leave the writer available.
async fn wait_for_latest_save(cx: &mut gpui::AsyncApp) -> anyhow::Result<()> {
    loop {
        let pending = cx.update(|cx| {
            cx.try_global::<AppLanguage>().and_then(|state| {
                state
                    .writer
                    .clone()
                    .map(|writer| (writer, state.latest_write))
            })
        })?;
        let Some((writer, sequence)) = pending else {
            return Ok(());
        };
        if let Err(error) = writer.flush().await {
            tracing::warn!(%error, "language save failed before quit");
        }
        if cx.update(|cx| cx.global::<AppLanguage>().latest_write == sequence)? {
            return Ok(());
        }
    }
}

pub(crate) fn quit_after_saving(cx: &mut App) {
    if let Some(state) = cx.try_global::<AppLanguage>() {
        if state.quit_pending {
            return;
        }
        cx.update_global::<AppLanguage, _>(|state, _| state.quit_pending = true);
    }
    cx.spawn(async move |cx| {
        if wait_for_latest_save(cx).await.is_err() {
            return;
        }
        let _ = cx.update(|cx| {
            if crate::workspace::sessions::can_quit(cx) {
                cx.quit();
            } else if cx.has_global::<AppLanguage>() {
                cx.update_global::<AppLanguage, _>(|state, _| state.quit_pending = false);
            }
        });
    })
    .detach();
}

pub(crate) fn select_language(preference: LanguagePreference, cx: &mut App) {
    let locale = preference.resolve(&system_languages());
    if let Some(language) = cx.try_global::<AppLanguage>() {
        if language.preference == preference && language.locale == locale {
            return;
        }
    }
    cx.update_global::<AppLanguage, _>(|language, _| {
        language.preference = preference;
        language.locale = locale;
        language.save_error = None;
        if let Some(writer) = &language.writer {
            match writer.select(preference) {
                Ok(sequence) => language.latest_write = Some(sequence),
                Err(reason) => {
                    language.save_error = Some(
                        LocalizedText::new("general.save_failed")
                            .with_arg("path", writer.path.display())
                            .with_arg("reason", reason),
                    )
                }
            }
        } else {
            language.save_error = Some(LocalizedText::new("general.storage_unavailable"));
        }
    });
    ui::i18n::set_app_locale(locale, cx);
    cx.set_menus(crate::menu_bar::menu_bar());
    cx.refresh_windows();
}

fn system_languages() -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        objc2_foundation::NSLocale::preferredLanguages()
            .iter()
            .map(|value| value.to_string())
            .collect()
    }
    #[cfg(not(target_os = "macos"))]
    {
        vec!["en".into()]
    }
}

pub(super) fn general_settings(theme: Theme, cx: &mut Context<crate::app::MultisoulApp>) -> Div {
    let language = cx.try_global::<AppLanguage>();
    let preference = language.map(|state| state.preference).unwrap_or_default();
    let error = language.and_then(|state| state.save_error.clone());
    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(
            div()
                .text_size(px(15.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgba(theme.text_primary))
                .child(text("general.interface_language")),
        )
        .child(
            div().flex().flex_col().gap(px(4.0)).children(
                LanguagePreference::ALL
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| {
                        Radio::new(("settings-interface-language", index))
                            .label(value.label())
                            .checked(value == preference)
                            .w_full()
                            .p(px(12.0))
                            .rounded(px(8.0))
                            .hover(move |style| style.bg(rgba((theme.accent & 0xffffff00) | 0x18)))
                            .active(move |style| style.bg(rgba((theme.accent & 0xffffff00) | 0x30)))
                            .tab_index(index as isize)
                            .on_key_down(move |event, _, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space")
                                    && !event.keystroke.modifiers.modified()
                                {
                                    select_language(value, cx);
                                    cx.stop_propagation();
                                }
                            })
                            .on_click(move |_, _, cx| select_language(value, cx))
                    }),
            ),
        )
        .child(
            div()
                .text_size(px(13.0))
                .text_color(rgba(theme.text_secondary))
                .child(text("general.language_hint")),
        )
        .when_some(error, |view, error| {
            view.child(
                div()
                    .text_size(px(13.0))
                    .text_color(rgba(theme.destructive))
                    .child(error.to_string()),
            )
        })
}

#[cfg(test)]
#[path = "../../tests/unit/language_settings_tests.rs"]
mod tests;
