//! `MultisoulApp` wallpaper action handlers (Task 8).
//!
//! Split out of `app/mod.rs` to respect the 500-line file cap. Currently hosts
//! the import action, which drives the rfd file picker and copies the chosen
//! file into the custom wallpapers dir via `WallpaperSettingsState::import_custom`.

use crate::app::file_picker::pick_wallpaper_image;
use crate::app::MultisoulApp;
use gpui::Context;

impl MultisoulApp {
    pub(crate) fn wallpaper_selection_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            let selection = self.wallpaper_settings.selection.clone();
            host.update(cx, |host, cx| {
                host.wallpaper_settings.selection = selection;
                host.sync_canvas_video_wallpaper(cx);
                cx.notify();
            });
        } else {
            self.sync_canvas_video_wallpaper(cx);
        }
        cx.notify();
    }

    /// Open the wallpaper file picker and, on a successful pick, import the
    /// chosen file as a custom wallpaper.
    ///
    /// The picker runs as an async task on the GPUI foreground executor: rfd's
    /// `AsyncFileDialog` marshals the native panel onto the macOS main run
    /// loop, so this does not block input draining. On a successful pick the
    /// file is copied into the custom dir and the selection is updated; the
    /// view refreshes via `cx.notify()`.
    pub(crate) fn import_wallpaper_file(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let Some(path) = pick_wallpaper_image().await else {
                return;
            };
            this.update(cx, |app, cx| {
                let result = app.wallpaper_settings.import_custom(&path);
                match &result {
                    crate::settings::wallpaper::ImportResult::Imported(name) => {
                        tracing::info!(?path, name, "imported custom wallpaper");
                    }
                    crate::settings::wallpaper::ImportResult::Rejected(reason) => {
                        tracing::warn!(?path, reason, "wallpaper import rejected");
                    }
                }
                app.wallpaper_selection_changed(cx);
            })
            .ok();
        })
        .detach();
    }
}
