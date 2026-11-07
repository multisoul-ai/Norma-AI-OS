//! App-wide island preferences. Loading and the ordered writer stay off the UI thread.
use gpui::{App, AppContext, Entity, Global};
use runtime::island::IslandPreferences;
use storage::SettingsStore;
use ui::i18n::LocalizedText;

mod persistence;
use persistence::Persistence;

pub(super) const KEY: &str = "island.preferences.v1";

pub(super) struct Prepared {
    preferences: IslandPreferences,
    store: Option<SettingsStore>,
    error: Option<LocalizedText>,
}

impl Prepared {
    fn load(store: SettingsStore) -> Self {
        let result = store
            .get_string(KEY)
            .map_err(|error| error.to_string())
            .and_then(|raw| {
                raw.map(|raw| serde_json::from_str(&raw).map_err(|error| error.to_string()))
                    .transpose()
            });
        match result {
            Ok(preferences) => Self {
                preferences: preferences.unwrap_or_default(),
                store: Some(store),
                error: None,
            },
            Err(reason) => Self {
                preferences: IslandPreferences::default(),
                store: Some(store),
                error: Some(
                    LocalizedText::new("island.settings_load_failed").with_arg("reason", reason),
                ),
            },
        }
    }
}

pub(crate) struct Settings {
    pub preferences: IslandPreferences,
    pub error: Option<LocalizedText>,
    /// Outcome of the last "clear remembered approvals" action.
    pub notice: Option<LocalizedText>,
    writer: Option<Persistence>,
    revision: Option<u64>,
}
struct SharedSettings(Entity<Settings>);
impl Global for SharedSettings {}

pub(crate) fn state(cx: &App) -> Option<Entity<Settings>> {
    cx.try_global::<SharedSettings>()
        .map(|global| global.0.clone())
}

/// Settings note for a finished system-notification authorization request; `None` when granted.
pub(super) fn notification_permission_notice(granted: bool, failed: bool) -> Option<LocalizedText> {
    (!granted || failed).then(|| LocalizedText::new("island.notify_permission_denied"))
}

/// Shows the permission note in the island settings (a hidden island depends on notifications).
pub(super) fn report_notification_permission(granted: bool, failed: bool, cx: &mut App) {
    let Some(entity) = state(cx) else { return };
    entity.update(cx, |state, cx| {
        state.notice = notification_permission_notice(granted, failed);
        cx.notify();
    });
}

pub(crate) fn current(cx: &App) -> IslandPreferences {
    state(cx)
        .map(|state| state.read(cx).preferences)
        .unwrap_or_default()
}

pub(crate) fn revision(cx: &App) -> Option<u64> {
    state(cx).and_then(|state| state.read(cx).revision)
}

pub(crate) fn install(cx: &mut App) {
    let load = cx.background_executor().spawn(async {
        match crate::database::default_database_context() {
            Ok((_, store)) => Prepared::load(store.settings_store()),
            Err(reason) => Prepared {
                preferences: IslandPreferences::default(),
                store: None,
                error: Some(
                    LocalizedText::new("island.settings_load_failed").with_arg("reason", reason),
                ),
            },
        }
    });
    cx.spawn(async move |cx| {
        let prepared = load.await;
        let _ = cx.update(|cx| {
            install_prepared(prepared, cx);
            super::windows::install(cx);
        });
    })
    .detach();
}

fn install_prepared(prepared: Prepared, cx: &mut App) {
    if state(cx).is_some() {
        return;
    }
    let writer = prepared.store.map(Persistence::start);
    let results = writer.as_ref().map(|writer| writer.results.clone());
    let entity = cx.new(|_| Settings {
        preferences: prepared.preferences,
        error: prepared.error,
        notice: None,
        writer,
        revision: None,
    });
    cx.set_global(SharedSettings(entity.clone()));
    if let Some(results) = results {
        let entity = entity.downgrade();
        cx.spawn(async move |cx| {
            while let Ok(result) = results.recv().await {
                if entity
                    .update(cx, |state, cx| {
                        if state.revision == Some(result.revision) {
                            state.error = result.result.err().map(|reason| {
                                LocalizedText::new("island.settings_save_failed")
                                    .with_arg("reason", reason)
                            });
                            cx.notify();
                            cx.refresh_windows();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }
}

/// Forget every remembered "always allow" grant in all retained workspace databases.
pub(crate) fn clear_remembered(cx: &mut App) {
    let Some(entity) = state(cx) else { return };
    let databases: std::collections::BTreeSet<_> = super::model(cx)
        .read(cx)
        .workspaces
        .iter()
        .map(|workspace| workspace.database.clone())
        .collect();
    let work = cx.background_executor().spawn(async move {
        databases
            .into_iter()
            .map(|database| SettingsStore::new(database).delete_prefix(super::remembered::PREFIX))
            .try_fold(0usize, |total, removed| removed.map(|n| total + n))
            .map_err(|error| error.to_string())
    });
    cx.spawn(async move |cx| {
        let result = work.await;
        let _ = entity.update(cx, |state, cx| {
            state.notice = Some(match result {
                Ok(count) => LocalizedText::new("island.remembered_cleared")
                    .with_arg("count", count.to_string()),
                Err(reason) => {
                    LocalizedText::new("island.remembered_clear_failed").with_arg("reason", reason)
                }
            });
            cx.notify();
            cx.refresh_windows();
        });
    })
    .detach();
}

#[derive(Clone, Copy)]
pub(crate) enum Setting {
    Hidden,
    Approvals,
    Questions,
    Completions,
}

pub(crate) fn select(setting: Setting, checked: bool, cx: &mut App) {
    let Some(entity) = state(cx) else { return };
    entity.update(cx, |state, cx| {
        let target = match setting {
            Setting::Hidden => &mut state.preferences.hidden,
            Setting::Approvals => &mut state.preferences.approvals,
            Setting::Questions => &mut state.preferences.questions,
            Setting::Completions => &mut state.preferences.completions,
        };
        if *target == checked {
            return;
        }
        *target = checked;
        state.error = None;
        match state
            .writer
            .as_ref()
            .map(|writer| writer.save(state.preferences))
        {
            Some(Ok(revision)) => state.revision = Some(revision),
            Some(Err(reason)) => {
                state.error = Some(
                    LocalizedText::new("island.settings_save_failed").with_arg("reason", reason),
                )
            }
            None => state.error = Some(LocalizedText::new("general.storage_unavailable")),
        }
        cx.notify();
        cx.refresh_windows();
    });
}

/// Wait before the App's final 100ms shutdown budget, including changes made while waiting.
pub(crate) async fn flush(cx: &mut gpui::AsyncApp) -> anyhow::Result<()> {
    loop {
        let pending = cx.update(|cx| {
            state(cx).and_then(|entity| {
                let state = entity.read(cx);
                state.writer.clone().map(|writer| (writer, state.revision))
            })
        })?;
        let Some((writer, revision)) = pending else {
            return Ok(());
        };
        if let Err(error) = writer.flush().await {
            tracing::warn!(%error, "island preferences could not be saved before quit");
        }
        if cx.update(|cx| state(cx).is_none_or(|entity| entity.read(cx).revision == revision))? {
            return Ok(());
        }
    }
}

#[cfg(test)]
pub(crate) fn install_for_test(preferences: IslandPreferences, cx: &mut App) {
    install_prepared(
        Prepared {
            preferences,
            store: None,
            error: None,
        },
        cx,
    );
}

#[cfg(test)]
pub(crate) fn install_paused_for_test(
    store: SettingsStore,
    cx: &mut App,
) -> (Persistence, std::sync::mpsc::Sender<()>) {
    install_prepared(Prepared::load(store), cx);
    let writer = state(cx).unwrap().read(cx).writer.clone().unwrap();
    let resume = writer.pause();
    select(Setting::Hidden, true, cx);
    (writer, resume)
}

#[cfg(test)]
#[path = "../../tests/unit/island_settings_tests.rs"]
mod tests;
