use crate::app::{file_picker::pick_wallpaper_image, MultisoulApp};
use crate::settings::wallpaper::state::WallpaperSelection;
use crate::settings::wallpaper::{
    preferences::WallpaperDeletePrompt, ImportResult, WallpaperSettingsState,
};
use gpui::{AppContext as _, Context, Window};
use storage::WallpaperReference;

impl MultisoulApp {
    pub(crate) fn import_wallpaper_file(&mut self, cx: &mut Context<Self>) {
        let host = crate::app::appearance::coordinator(self, cx).unwrap_or_else(|| cx.entity());
        cx.defer(move |cx| {
            host.update(cx, |app, cx| {
                if app.wallpaper_settings.preferences.busy
                    || app.wallpaper_settings.preferences.delete_prompt.is_some()
                {
                    return;
                }
                app.wallpaper_settings.preferences.busy = true;
                app.wallpaper_settings.preferences.revision += 1;
                app.broadcast_wallpaper_state(
                    Some(ui::i18n::message("settings.wallpaper_importing", &[])),
                    cx,
                );
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
                        app.wallpaper_settings.preferences.busy = false;
                        app.wallpaper_settings.preferences.revision += 1;
                        match result {
                            Some(ImportResult::Imported(filename)) => {
                                app.wallpaper_settings
                                    .preferences
                                    .customs
                                    .push(filename.clone());
                                app.wallpaper_settings.preferences.customs.sort();
                                app.save_wallpaper(WallpaperReference::Custom { filename }, cx);
                            }
                            Some(ImportResult::Rejected(reason)) => {
                                app.broadcast_wallpaper_state(
                                    Some(ui::i18n::message(
                                        "settings.wallpaper_import_failed",
                                        &[("error", reason)],
                                    )),
                                    cx,
                                );
                            }
                            None => app.broadcast_wallpaper_state(
                                Some(ui::i18n::message(
                                    "settings.wallpaper_import_cancelled",
                                    &[],
                                )),
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
        let host = crate::app::appearance::coordinator(self, cx).unwrap_or_else(|| cx.entity());
        cx.defer(move |cx| {
            host.update(cx, |app, cx| {
                if app.wallpaper_settings.preferences.busy
                    || app.wallpaper_settings.preferences.delete_prompt.is_some()
                {
                    return;
                }
                let prompt = WallpaperDeletePrompt { filename };
                let uses_wallpaper = app.wallpaper_settings.preferences.global.reference()
                    == WallpaperReference::Custom {
                        filename: prompt.filename.clone(),
                    };
                app.wallpaper_settings.preferences.delete_prompt = Some(prompt.clone());
                app.broadcast_wallpaper_state(None, cx);
                let host = cx.entity();
                cx.defer(move |cx| {
                    if cx
                        .update_window(window_handle, |_, window, cx| {
                            let theme = host.read(cx).theme;
                            crate::settings::wallpaper::delete_dialog::open(
                                host.clone(),
                                prompt,
                                theme,
                                uses_wallpaper,
                                window,
                                cx,
                            );
                        })
                        .is_err()
                    {
                        host.update(cx, |app, cx| {
                            app.wallpaper_settings.preferences.delete_prompt = None;
                            app.broadcast_wallpaper_state(None, cx);
                        });
                    }
                });
            })
        });
    }

    pub(crate) fn finish_wallpaper_delete(&mut self, confirmed: bool, cx: &mut Context<Self>) {
        let host = crate::app::appearance::coordinator(self, cx).unwrap_or_else(|| cx.entity());
        cx.defer(move |cx| {
            host.update(cx, |app, cx| {
                if app.wallpaper_settings.preferences.busy {
                    return;
                }
                let Some(prompt) = app.wallpaper_settings.preferences.delete_prompt.take() else {
                    return;
                };
                if !confirmed {
                    app.broadcast_wallpaper_state(None, cx);
                    return;
                }
                app.wallpaper_settings.preferences.busy = true;
                app.wallpaper_settings.preferences.revision += 1;
                app.broadcast_wallpaper_state(None, cx);
                let catalog = app.desktop_workspace.store.clone();
                let filename = prompt.filename;
                let deleted = filename.clone();
                let work = cx.background_executor().spawn(async move {
                    catalog.clear_custom_wallpaper(
                        &filename,
                        &WallpaperSelection::default().reference(),
                    )?;
                    let error = crate::settings::wallpaper::dirs::custom_wallpapers_dir()
                        .map(|dir| std::fs::remove_file(dir.join(&filename)))
                        .transpose()
                        .err()
                        .filter(|error| error.kind() != std::io::ErrorKind::NotFound)
                        .map(|error| error.to_string());
                    Ok::<_, storage::StorageError>(error)
                });
                cx.spawn(async move |this, cx| {
                    let result = work.await;
                    let _ = this.update(cx, |app, cx| {
                        app.wallpaper_settings.preferences.busy = false;
                        app.wallpaper_settings.preferences.revision += 1;
                        match result {
                            Ok(file_error) => {
                                let custom = WallpaperReference::Custom {
                                    filename: deleted.clone(),
                                };
                                if app.wallpaper_settings.preferences.global.reference() == custom {
                                    app.wallpaper_settings.preferences.global =
                                        WallpaperSelection::default();
                                }
                                if file_error.is_none() {
                                    app.wallpaper_settings
                                        .preferences
                                        .customs
                                        .retain(|filename| filename != &deleted);
                                }
                                app.bind_desktop_wallpaper(cx);
                                let message = file_error.map_or_else(
                                    || ui::i18n::message("settings.wallpaper_deleted", &[]),
                                    |error| {
                                        ui::i18n::message(
                                            "settings.wallpaper_file_delete_failed",
                                            &[("error", error)],
                                        )
                                    },
                                );
                                app.broadcast_wallpaper_state(Some(message), cx);
                            }
                            Err(error) => {
                                app.wallpaper_settings.preferences.status =
                                    Some(ui::i18n::message(
                                        "settings.wallpaper_delete_failed",
                                        &[("error", error.to_string())],
                                    ));
                                app.broadcast_wallpaper_state(None, cx);
                            }
                        }
                    });
                })
                .detach();
            })
        });
    }
}

impl MultisoulApp {
    pub(crate) fn refresh_wallpaper_library(&mut self, cx: &mut Context<Self>) {
        let host = crate::app::appearance::coordinator(self, cx).unwrap_or_else(|| cx.entity());
        cx.defer(move |cx| {
            host.update(cx, |app, cx| {
                let revision = app.wallpaper_settings.preferences.revision;
                let state = app.wallpaper_settings.clone();
                let work = cx
                    .background_executor()
                    .spawn(async move { state.list_custom() });
                cx.spawn(async move |this, cx| {
                    let customs = work.await;
                    let _ = this.update(cx, |app, cx| {
                        if app.wallpaper_settings.preferences.revision != revision {
                            return;
                        }
                        app.wallpaper_settings.preferences.customs = customs;
                        app.bind_desktop_wallpaper(cx);
                        app.broadcast_wallpaper_state(None, cx);
                    });
                })
                .detach();
            })
        });
    }
}
