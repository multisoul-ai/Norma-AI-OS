//! Global wallpaper actions. The desktop host serializes writes across project views.
mod assets;
mod sync;

use crate::app::MultisoulApp;
use crate::settings::wallpaper::state::WallpaperSelection;
use gpui::Context;
use storage::WallpaperReference;

impl MultisoulApp {
    pub(crate) fn request_wallpaper_selection(
        &mut self,
        selection: WallpaperSelection,
        cx: &mut Context<Self>,
    ) {
        if let WallpaperSelection::Builtin { rel } = &selection {
            crate::media::request(rel, true, cx);
        }
        self.queue_wallpaper_save(selection.reference(), cx);
    }

    fn queue_wallpaper_save(&self, reference: WallpaperReference, cx: &mut Context<Self>) {
        let host = crate::app::appearance::coordinator(self, cx).unwrap_or_else(|| cx.entity());
        // A project may be rendered while the host is borrowed. Apply on the next UI tick.
        cx.defer(move |cx| host.update(cx, |app, cx| app.save_wallpaper(reference, cx)));
    }

    fn save_wallpaper(&mut self, reference: WallpaperReference, cx: &mut Context<Self>) {
        if self.wallpaper_settings.preferences.busy
            || self.wallpaper_settings.preferences.delete_prompt.is_some()
        {
            return;
        }
        let previous = self.wallpaper_settings.preferences.global.reference();
        self.wallpaper_settings.preferences.busy = true;
        crate::telemetry::feature_used(analytics::Feature::Wallpaper);
        self.wallpaper_settings.preferences.revision += 1;
        self.publish_wallpaper(
            reference.clone(),
            Some(ui::i18n::message("norma.saving", &[])),
            cx,
        );
        let settings = storage::SettingsStore::new(self.database_paths.app_db_path().to_owned());
        let saved = reference.clone();
        let work = cx.background_executor().spawn(async move {
            reference.validate()?;
            let (kind, value) = match &reference {
                WallpaperReference::Builtin { rel } => ("builtin", rel),
                WallpaperReference::Custom { filename } => ("custom", filename),
            };
            settings.apply_string_changes(&[
                ("wallpaper.kind".into(), Some(kind.into())),
                ("wallpaper.value".into(), Some(value.clone())),
            ])
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                app.wallpaper_settings.preferences.busy = false;
                app.wallpaper_settings.preferences.revision += 1;
                match result {
                    Ok(()) => app.publish_wallpaper(
                        saved,
                        Some(ui::i18n::message("norma.saved", &[])),
                        cx,
                    ),
                    Err(error) => {
                        tracing::warn!(%error, "wallpaper save failed");
                        app.publish_wallpaper(
                            previous,
                            Some(ui::i18n::message("settings.wallpaper_save_failed", &[])),
                            cx,
                        );
                    }
                }
            });
        })
        .detach();
    }
}
