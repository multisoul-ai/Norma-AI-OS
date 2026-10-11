//! Scoped wallpaper actions. The desktop host serializes writes across project views.
mod assets;
mod sync;

use crate::app::MultisoulApp;
use crate::settings::wallpaper::scoped::WallpaperTarget;
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
        let target = self.wallpaper_settings.scoped.target();
        let previous = self.wallpaper_settings.scoped.saved_reference();
        self.queue_wallpaper_save(target, previous, Some(selection.reference()), cx);
    }

    pub(crate) fn set_wallpaper_follow_workspace(&mut self, follows: bool, cx: &mut Context<Self>) {
        let target = self.wallpaper_settings.scoped.target();
        if !matches!(target, WallpaperTarget::Project { .. }) {
            return;
        }
        let previous = self.wallpaper_settings.scoped.saved_reference();
        let reference = (!follows).then(|| {
            self.wallpaper_settings
                .scoped
                .workspace_selection()
                .reference()
        });
        self.queue_wallpaper_save(target, previous, reference, cx);
    }

    fn queue_wallpaper_save(
        &self,
        target: WallpaperTarget,
        previous: Option<WallpaperReference>,
        reference: Option<WallpaperReference>,
        cx: &mut Context<Self>,
    ) {
        let host = self
            .desktop_host
            .as_ref()
            .and_then(|host| host.upgrade())
            .unwrap_or_else(|| cx.entity());
        // A project may be rendered while the host is borrowed. Apply on the next UI tick.
        cx.defer(move |cx| {
            host.update(cx, |app, cx| {
                app.save_wallpaper(target, previous, reference, cx)
            })
        });
    }

    fn save_wallpaper(
        &mut self,
        target: WallpaperTarget,
        previous: Option<WallpaperReference>,
        reference: Option<WallpaperReference>,
        cx: &mut Context<Self>,
    ) {
        if self.wallpaper_settings.scoped.busy
            || self.wallpaper_settings.scoped.delete_prompt.is_some()
        {
            return;
        }
        self.wallpaper_settings.scoped.busy = true;
        self.wallpaper_settings.scoped.revision += 1;
        self.publish_wallpaper(&target, reference.clone(), Some("Saving…".into()), cx);
        let catalog = self.desktop_workspace.store.clone();
        let settings = storage::SettingsStore::new(self.database_paths.app_db_path().to_owned());
        let destination = target.clone();
        let saved = reference.clone();
        let work = cx.background_executor().spawn(async move {
            match &destination {
                WallpaperTarget::Workspace(id) => catalog.set_workspace_wallpaper(
                    id,
                    reference
                        .as_ref()
                        .ok_or(storage::StorageError::InvalidWallpaperReference)?,
                ),
                WallpaperTarget::Project {
                    workspace_id,
                    area_id,
                } => catalog.set_project_wallpaper(workspace_id, area_id, reference.as_ref()),
                WallpaperTarget::Global => {
                    let reference = reference
                        .as_ref()
                        .ok_or(storage::StorageError::InvalidWallpaperReference)?;
                    reference.validate()?;
                    let (kind, value) = match reference {
                        WallpaperReference::Builtin { rel } => ("builtin", rel),
                        WallpaperReference::Custom { filename } => ("custom", filename),
                    };
                    settings.apply_string_changes(&[
                        ("wallpaper.kind".into(), Some(kind.into())),
                        ("wallpaper.value".into(), Some(value.clone())),
                    ])
                }
            }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                app.wallpaper_settings.scoped.busy = false;
                app.wallpaper_settings.scoped.revision += 1;
                match result {
                    Ok(()) => app.publish_wallpaper(&target, saved, Some("Saved".into()), cx),
                    Err(error) => {
                        tracing::warn!(%error, ?target, "wallpaper save failed");
                        app.publish_wallpaper(&target, previous, Some("Couldn't save wallpaper. The previous selection has been restored.".into()), cx);
                    }
                }
            });
        }).detach();
    }
}
