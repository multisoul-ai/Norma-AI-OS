//! App-wide Quit precedes focused widgets' keymaps and retains the original shutdown guard.
use crate::{
    app::{MultisoulApp, QuitNormaOs},
    shortcuts::{ShortcutCommandId, ShortcutSettingsState},
};
use gpui::App;

pub(crate) fn install(cx: &mut App) {
    let defaults = ShortcutSettingsState::default();
    cx.intercept_keystrokes(move |event, window, cx| {
        let target = window
            .root::<ui::component::Root>()
            .flatten()
            .and_then(|root| root.read(cx).view().clone().downcast::<MultisoulApp>().ok());
        let should_quit = if let Some(target) = target {
            let app = target.read(cx);
            app.shortcuts.command_for_keystroke(&event.keystroke)
                == Some(ShortcutCommandId::QuitNormaOs)
                && app.shortcuts.recording.is_none()
                && !app
                    .expanded_desktop_project()
                    .is_some_and(|project| project.read(cx).shortcuts.recording.is_some())
        } else {
            defaults.command_for_keystroke(&event.keystroke) == Some(ShortcutCommandId::QuitNormaOs)
        };
        if should_quit {
            cx.stop_propagation();
            if !event.is_held {
                cx.defer(|cx| cx.dispatch_action(&QuitNormaOs));
            }
        }
    })
    .detach();
}
