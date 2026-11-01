use crate::app::MultisoulApp;
use gpui::Context;
use storage::WallpaperReference;

impl MultisoulApp {
    pub(crate) fn bind_desktop_wallpaper(&mut self, cx: &mut Context<Self>) {
        let selection = self.wallpaper_settings.preferences.effective_selection();
        self.wallpaper_settings
            .set_effective_selection(selection, self.motion.reduce_motion);
        self.refresh_optional_media(cx);
        self.sync_all_dispatch_effects(cx);
    }

    pub(super) fn publish_wallpaper(
        &mut self,
        reference: WallpaperReference,
        status: Option<ui::i18n::LocalizedText>,
        cx: &mut Context<Self>,
    ) {
        self.wallpaper_settings.preferences.global = self
            .wallpaper_settings
            .preferences
            .resolve(&reference)
            .unwrap_or_default();
        self.bind_desktop_wallpaper(cx);
        self.broadcast_wallpaper_state(status, cx);
    }

    pub(crate) fn broadcast_wallpaper_state(
        &mut self,
        status: Option<ui::i18n::LocalizedText>,
        cx: &mut Context<Self>,
    ) {
        if let Some(status) = status {
            self.wallpaper_settings.preferences.status = Some(status);
        }
        let preferences = self.wallpaper_settings.preferences.clone();
        let database = self.database_paths.app_db_path().to_owned();
        let sender = cx.entity();
        cx.defer(move |cx| {
            for view in crate::app::appearance::views(&database, sender, cx) {
                view.update(cx, |app, cx| {
                    app.wallpaper_settings.preferences = preferences.clone();
                    let selection = preferences.effective_selection();
                    app.wallpaper_settings
                        .set_effective_selection(selection, app.motion.reduce_motion);
                    app.refresh_optional_media(cx);
                    cx.notify();
                });
            }
        });
        cx.notify();
    }
}
