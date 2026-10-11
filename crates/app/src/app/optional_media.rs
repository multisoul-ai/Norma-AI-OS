use super::MultisoulApp;
use crate::settings::wallpaper::state::WallpaperSelection;
use gpui::Context;

impl MultisoulApp {
    /// Download completion only refreshes the currently effective selection.
    /// It never writes wallpaper scope, switches guide topic, or opens a view.
    pub(crate) fn refresh_optional_media(&mut self, cx: &mut Context<Self>) {
        if let WallpaperSelection::Builtin { rel } = &self.wallpaper_settings.selection {
            crate::media::request(rel, false, cx);
        }
        #[cfg(target_os = "macos")]
        self.sync_canvas_video_wallpaper(cx);
        if self.feature_guide.is_visible() {
            self.sync_feature_guide_media(cx);
        }
    }
}
