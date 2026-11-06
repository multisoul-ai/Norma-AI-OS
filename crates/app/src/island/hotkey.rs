//! System-wide island shortcut: Carbon registration follows the Settings shortcut.
mod carbon;
mod chord;
pub(crate) use chord::Chord;

use crate::shortcuts::{ShortcutCommandId, ShortcutSettingsState};
use gpui::{App, Global};
use ui::i18n::LocalizedText;

#[derive(Default)]
struct Registered(Option<(Chord, carbon::Registration)>);
impl Global for Registered {}

/// Installs the press listener, then registers the persisted shortcut from a background load.
pub(crate) fn install(cx: &mut App) {
    cx.set_global(Registered::default());
    let presses = match carbon::install() {
        Ok(presses) => presses,
        Err(code) => {
            tracing::warn!(code, "island hot-key handler could not be installed");
            return;
        }
    };
    cx.spawn(async move |cx| {
        while presses.recv().await.is_ok() {
            if cx.update(super::windows::toggle).is_err() {
                break;
            }
        }
    })
    .detach();
    let load = cx.background_executor().spawn(async {
        let (_, store) = crate::database::default_database_context().ok()?;
        ShortcutSettingsState::load_from_store(store.settings_store())
            .current_keystroke(ShortcutCommandId::ToggleIsland)
            .map(str::to_owned)
    });
    cx.spawn(async move |cx| {
        let source = load.await;
        let _ = cx.update(|cx| {
            if let Err(error) = sync(source.as_deref(), cx) {
                tracing::warn!(?error, "island hot-key could not be registered at launch");
            }
        });
    })
    .detach();
}

/// Re-registers the system shortcut. `None` or an unsupported chord leaves it unregistered.
pub(crate) fn sync(source: Option<&str>, cx: &mut App) -> Result<(), LocalizedText> {
    let wanted = source.map(Chord::parse).transpose();
    let global = cx.default_global::<Registered>();
    let wanted = match wanted {
        Ok(wanted) => wanted,
        Err(_) => {
            global.0 = None;
            return Err(LocalizedText::new(
                "shortcuts.global_shortcut_needs_modifier",
            ));
        }
    };
    if global.0.as_ref().map(|(chord, _)| *chord) == wanted {
        return Ok(());
    }
    // Carbon rejects a duplicate chord, so release the old one before registering the new one.
    let previous = global.0.take();
    let Some(chord) = wanted else { return Ok(()) };
    match carbon::Registration::new(chord) {
        Ok(registration) => {
            global.0 = Some((chord, registration));
            Ok(())
        }
        Err(code) => {
            if let Some((old, _)) = previous {
                global.0 = carbon::Registration::new(old).ok().map(|r| (old, r));
            }
            Err(LocalizedText::new("island.hotkey_unavailable")
                .with_arg("shortcut", crate::shortcuts::keystroke_label(source))
                .with_arg("code", code.to_string()))
        }
    }
}
