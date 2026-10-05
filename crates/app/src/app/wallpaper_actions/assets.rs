use crate::app::{file_picker::pick_wallpaper_image, MultisoulApp};
use crate::settings::wallpaper::state::WallpaperSelection;
use crate::settings::wallpaper::{
    scoped::WallpaperDeletePrompt, ImportResult, WallpaperSettingsState,
};
use gpui::{AppContext as _, Context, Window};
use storage::WallpaperReference;

impl MultisoulApp {
    pub(crate) fn import_wallpaper_file(&mut self, cx: &mut Context<Self>) {
        let target = self.wallpaper_settings.scoped.target();
        let previous = self.wallpaper_settings.scoped.saved_reference();
        let host = self
            .desktop_host
            .as_ref()
            .and_then(|host| host.upgrade())
            .unwrap_or_else(|| cx.entity());
        cx.defer(move |cx| {
            host.update(cx, |app, cx| {
                if app.wallpaper_settings.scoped.busy {
                    return;
                }
                app.wallpaper_settings.scoped.busy = true;
                app.wallpaper_settings.scoped.revision += 1;
                app.broadcast_wallpaper_state(Some((&target, "Importing…".into())), cx);
                cx.spawn(async move |this, cx| {
                    let result = if let Some(path) = pick_wallpaper_image().await {
                        Some(
                            cx.background_executor()
                                .spawn(async move { WallpaperSettingsState::copy_custom(&path) })
                                .await,
                        )
                    } else {
                        None
                    };
                    let _ = this.update(cx, |app, cx| {
                        app.wallpaper_settings.scoped.busy = false;
                        app.wallpaper_settings.scoped.revision += 1;
                        match result {
                            Some(ImportResult::Imported(filename)) => {
                                app.wallpaper_settings.scoped.customs.push(filename.clone());
                                app.wallpaper_settings.scoped.customs.sort();
                                app.save_wallpaper(
                                    target,
                                    previous,
                                    Some(WallpaperReference::Custom { filename }),
                                    cx,
                                );
                            }
                            Some(ImportResult::Rejected(reason)) => {
                                app.broadcast_wallpaper_state(
                                    Some((&target, format!("Couldn't import wallpaper: {reason}"))),
                                    cx,
                                );
                            }
                            None => app.broadcast_wallpaper_state(
                                Some((&target, "Import cancelled".into())),
                                cx,
                            ),
                        }
                    });
                })
                .detach();
            })
        });
    }

    pub(crate) fn request_wallpaper_delete(
        &mut self,
        filename: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let window_handle = window.window_handle();
        let host = self
            .desktop_host
            .as_ref()
            .and_then(|host| host.upgrade())
            .unwrap_or_else(|| cx.entity());
        cx.defer(move |cx| {
            host.update(cx, |app, cx| {
                if app.wallpaper_settings.scoped.busy {
                    return;
                }
                app.wallpaper_settings.scoped.busy = true;
                app.wallpaper_settings.scoped.revision += 1;
                app.broadcast_wallpaper_state(None, cx);
                let store = app.desktop_workspace.store.clone();
                let work = cx.background_executor().spawn(async move {
                    store
                        .wallpaper_usage(&filename)
                        .map(|usage| WallpaperDeletePrompt { filename, usage })
                });
                cx.spawn(async move |this, cx| {
                    let result = work.await;
                    let displayed = cx.update_window(window_handle, |_, window, cx| {
                        this.update(cx, |app, cx| {
                            app.wallpaper_settings.scoped.busy = false;
                            app.wallpaper_settings.scoped.revision += 1;
                            match result {
                                Ok(prompt) => {
                                    let directory_uses_wallpaper =
                                        app.wallpaper_settings.scoped.global.reference()
                                            == (WallpaperReference::Custom {
                                                filename: prompt.filename.clone(),
                                            });
                                    app.wallpaper_settings.scoped.delete_prompt =
                                        Some(prompt.clone());
                                    crate::settings::wallpaper::delete_dialog::open(
                                        cx.entity(),
                                        prompt,
                                        app.theme,
                                        directory_uses_wallpaper,
                                        window,
                                        cx,
                                    );
                                }
                                Err(error) => {
                                    app.wallpaper_settings.scoped.status = Some(format!(
                                        "Couldn't check wallpaper references: {error}"
                                    ))
                                }
                            }
                            app.broadcast_wallpaper_state(None, cx);
                        })
                    });
                    if displayed.is_err() {
                        let _ = this.update(cx, |app, cx| {
                            app.wallpaper_settings.scoped.busy = false;
                            app.wallpaper_settings.scoped.revision += 1;
                            app.wallpaper_settings.scoped.delete_prompt = None;
                            app.broadcast_wallpaper_state(None, cx);
                        });
                    }
                })
                .detach();
            })
        });
    }

    pub(crate) fn finish_wallpaper_delete(&mut self, confirmed: bool, cx: &mut Context<Self>) {
        let host = self
            .desktop_host
            .as_ref()
            .and_then(|host| host.upgrade())
            .unwrap_or_else(|| cx.entity());
        cx.defer(move |cx| host.update(cx, |app, cx| {
            if app.wallpaper_settings.scoped.busy { return; }
            let Some(prompt) = app.wallpaper_settings.scoped.delete_prompt.take() else { return; };
            if !confirmed { app.broadcast_wallpaper_state(None, cx); return; }
            app.wallpaper_settings.scoped.busy = true;
            app.wallpaper_settings.scoped.revision += 1;
            app.broadcast_wallpaper_state(None, cx);
            let catalog = app.desktop_workspace.store.clone();
            let filename = prompt.filename;
            let deleted = filename.clone();
            let work = cx.background_executor().spawn(async move {
                catalog.clear_custom_wallpaper(&filename, &WallpaperSelection::default().reference())?;
                let error = crate::settings::wallpaper::dirs::custom_wallpapers_dir()
                    .map(|dir| std::fs::remove_file(dir.join(&filename)))
                    .transpose().err().filter(|error| error.kind() != std::io::ErrorKind::NotFound)
                    .map(|error| error.to_string());
                Ok::<_, storage::StorageError>(error)
            });
            cx.spawn(async move |this, cx| {
                let result = work.await;
                let _ = this.update(cx, |app, cx| {
                    app.wallpaper_settings.scoped.busy = false;
                    app.wallpaper_settings.scoped.revision += 1;
                    match result {
                        Ok(file_error) => {
                            let custom = WallpaperReference::Custom { filename: deleted.clone() };
                            let default = WallpaperSelection::default().reference();
                            for workspace in &mut app.desktop_workspace.workspaces {
                                if workspace.wallpaper.as_ref() == Some(&custom) { workspace.wallpaper = Some(default.clone()); }
                            }
                            for area in &mut app.desktop_workspace.areas {
                                if area.wallpaper.as_ref() == Some(&custom) { area.wallpaper = None; }
                            }
                            if app.wallpaper_settings.scoped.global.reference() == custom { app.wallpaper_settings.scoped.global = WallpaperSelection::default(); }
                            if file_error.is_none() { app.wallpaper_settings.scoped.customs.retain(|filename| filename != &deleted); }
                            for session in app.desktop_workspace.sessions.values().cloned().collect::<Vec<_>>() {
                                session.update(cx, |session, _| {
                                    if let Some(area) = session.wallpaper_settings.scoped.context.as_mut().and_then(|context| context.area.as_mut()) {
                                        if area.wallpaper.as_ref() == Some(&custom) { area.wallpaper = None; }
                                    }
                                });
                            }
                            app.bind_desktop_wallpaper(cx);
                            let message = file_error.map_or_else(|| "Wallpaper deleted".into(), |error| format!("References reset, but the file couldn't be deleted: {error}"));
                            let target = app.wallpaper_settings.scoped.target();
                            app.broadcast_wallpaper_state(Some((&target, message)), cx);
                        }
                        Err(error) => {
                            app.wallpaper_settings.scoped.status = Some(format!("Couldn't delete wallpaper: {error}"));
                            app.broadcast_wallpaper_state(None, cx);
                        }
                    }
                });
            }).detach();
        }));
    }
}

impl MultisoulApp {
    pub(crate) fn refresh_wallpaper_library(&mut self, cx: &mut Context<Self>) {
        let host = self
            .desktop_host
            .as_ref()
            .and_then(|host| host.upgrade())
            .unwrap_or_else(|| cx.entity());
        cx.defer(move |cx| {
            host.update(cx, |app, cx| {
                let revision = app.wallpaper_settings.scoped.revision;
                let state = app.wallpaper_settings.clone();
                let work = cx
                    .background_executor()
                    .spawn(async move { state.list_custom() });
                cx.spawn(async move |this, cx| {
                    let customs = work.await;
                    let _ = this.update(cx, |app, cx| {
                        if app.wallpaper_settings.scoped.revision != revision {
                            return;
                        }
                        app.wallpaper_settings.scoped.customs = customs;
                        app.bind_desktop_wallpaper(cx);
                        app.broadcast_wallpaper_state(None, cx);
                    });
                })
                .detach();
            })
        });
    }
}
