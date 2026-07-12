use super::{action_button, setting_label, with_alpha};
use crate::app::MultisoulApp;
use crate::settings::voice::TtsModelInstallState;
use gpui::{div, prelude::*, px, rgba, Context, Div};
use ui::Theme;

enum TtsInstallEvent {
    Progress(u64, u64),
    Finished(Result<(), String>),
}

pub(super) fn tts_catalog_row(
    theme: Theme,
    state: &crate::settings::voice::VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let selected = state.values().tts_model_id();
    let mut list = div().w_full().flex().flex_col().gap(px(8.0));
    for (index, entry) in state.tts_model_manager().entries().iter().enumerate() {
        let descriptor = entry.descriptor().clone();
        let id = descriptor.id().clone();
        let selected_here = selected == Some(&id);
        let action = match entry.installation() {
            TtsModelInstallState::NotInstalled => div().child(
                action_button(theme, "Install", theme.accent_orange)
                    .id(("settings-tts-install", index))
                    .debug_selector(move || format!("settings-tts-install-{index}"))
                    .on_click(
                        cx.listener(move |app, _, _, cx| app.install_tts_model(id.clone(), cx)),
                    ),
            ),
            TtsModelInstallState::Installing { downloaded, total } => div()
                .px(px(12.0))
                .py(px(8.0))
                .text_color(rgba(theme.accent_cyan))
                .child(format!(
                    "{}%",
                    if *total == 0 {
                        0
                    } else {
                        downloaded.saturating_mul(100) / total
                    }
                )),
            TtsModelInstallState::Ready if selected_here => {
                let preview_id = id.clone();
                div()
                    .flex()
                    .gap(px(6.0))
                    .child(
                        div()
                            .px(px(12.0))
                            .py(px(8.0))
                            .text_color(rgba(theme.accent_cyan))
                            .child("Selected"),
                    )
                    .child(
                        action_button(theme, "Preview", theme.accent_cyan)
                            .id(("settings-tts-preview", index))
                            .debug_selector(move || format!("settings-tts-preview-{index}"))
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.preview_tts_model(preview_id.clone(), cx)
                            })),
                    )
            }
            TtsModelInstallState::Ready => {
                let select_id = id.clone();
                let delete_id = id.clone();
                div()
                    .flex()
                    .gap(px(6.0))
                    .child(
                        action_button(theme, "Select", theme.accent_cyan)
                            .id(("settings-tts-select", index))
                            .debug_selector(move || format!("settings-tts-select-{index}"))
                            .on_click(cx.listener(move |app, _, _, cx| {
                                let _ = app.apply_tts_selection(select_id.clone(), None, cx);
                            })),
                    )
                    .child(
                        action_button(theme, "Delete", theme.destructive)
                            .id(("settings-tts-delete", index))
                            .debug_selector(move || format!("settings-tts-delete-{index}"))
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.delete_tts_model(delete_id.clone(), cx)
                            })),
                    )
            }
            TtsModelInstallState::Failed(_) => div().child(
                action_button(theme, "Retry", theme.destructive)
                    .id(("settings-tts-retry", index))
                    .debug_selector(move || format!("settings-tts-retry-{index}"))
                    .on_click(
                        cx.listener(move |app, _, _, cx| app.install_tts_model(id.clone(), cx)),
                    ),
            ),
        };
        let status = match entry.installation() {
            TtsModelInstallState::NotInstalled => "Not installed",
            TtsModelInstallState::Installing { .. } => "Installing",
            TtsModelInstallState::Ready => "Ready",
            TtsModelInstallState::Failed(_) => "Installation failed",
        };
        list = list.child(
            div()
                .id(("settings-tts-model", index))
                .debug_selector(move || format!("settings-tts-model-{index}"))
                .w_full()
                .max_w(px(620.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(16.0))
                .px(px(14.0))
                .py(px(12.0))
                .rounded(px(10.0))
                .border_1()
                .border_color(rgba(with_alpha(
                    if selected_here {
                        theme.accent_orange
                    } else {
                        theme.edge_highlight
                    },
                    if selected_here { 0x90 } else { 0x45 },
                )))
                .bg(rgba(with_alpha(theme.panel_high, 0xa8)))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_color(rgba(theme.text_primary))
                                .child(descriptor.display_name().to_string()),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgba(theme.text_tertiary))
                                .child(format!(
                                    "{} · {} MB · {status}",
                                    descriptor
                                        .capabilities()
                                        .languages()
                                        .join(" + ")
                                        .to_uppercase(),
                                    descriptor.package().download_bytes() / 1_000_000
                                )),
                        ),
                )
                .child(action),
        );
    }
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .py(px(6.0))
        .child(setting_label(
            theme,
            "Local TTS Model",
            "Download, select and remove verified local voices",
        ))
        .child(list)
}

impl MultisoulApp {
    pub(crate) fn refresh_tts_model_states(&mut self, cx: &mut Context<Self>) {
        let root = self.voice_settings.tts_model_root();
        let work = cx.background_executor().spawn(async move {
            crate::settings::voice::TtsModelManagerState::scan(&tts::builtin_model_catalog(), &root)
        });
        cx.spawn(async move |this, cx| {
            let manager = work.await;
            let _ = this.update(cx, |app, cx| {
                app.voice_settings.replace_tts_model_manager(manager);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn install_tts_model(&mut self, id: tts::TtsModelId, cx: &mut Context<Self>) {
        let catalog = tts::builtin_model_catalog();
        let Some(descriptor) = catalog.find(&id).cloned() else {
            return;
        };
        let root = self.voice_settings.tts_model_root();
        self.voice_settings
            .tts_model_manager_mut()
            .begin_install(&id, descriptor.package().download_bytes());
        let (tx, rx) = async_channel::bounded(32);
        cx.background_executor()
            .spawn(async move {
                let result = (|| {
                    let files = crate::settings::voice::tts_models::download_tts_package(
                        &descriptor,
                        &root.join(".downloads"),
                        |downloaded, total| {
                            let _ = tx.try_send(TtsInstallEvent::Progress(downloaded, total));
                        },
                    )?;
                    tts::install_downloaded_package(
                        &descriptor,
                        &files.archive,
                        &files.support,
                        &root,
                    )
                    .map_err(|error| error.to_string())?;
                    let _ = std::fs::remove_file(files.archive);
                    for file in files.support {
                        let _ = std::fs::remove_file(file);
                    }
                    Ok(())
                })();
                let _ = tx.send_blocking(TtsInstallEvent::Finished(result));
            })
            .detach();
        cx.spawn(async move |this, cx| {
            while let Ok(event) = rx.recv().await {
                let terminal = matches!(event, TtsInstallEvent::Finished(_));
                let _ = this.update(cx, |app, cx| {
                    match event {
                        TtsInstallEvent::Progress(downloaded, total) => app
                            .voice_settings
                            .tts_model_manager_mut()
                            .update_progress(&id, downloaded, total),
                        TtsInstallEvent::Finished(Ok(())) => {
                            app.voice_settings.tts_model_manager_mut().mark_ready(&id);
                            if app.voice_settings.values().tts_model_id().is_none() {
                                let _ = app.apply_tts_selection(id.clone(), None, cx);
                            }
                        }
                        TtsInstallEvent::Finished(Err(error)) => app
                            .voice_settings
                            .tts_model_manager_mut()
                            .mark_failed(&id, error),
                    };
                    cx.notify();
                });
                if terminal {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn delete_tts_model(&mut self, id: tts::TtsModelId, cx: &mut Context<Self>) {
        let catalog = tts::builtin_model_catalog();
        let Some(descriptor) = catalog.find(&id).cloned() else {
            return;
        };
        let root = self.voice_settings.tts_model_root().join(id.as_str());
        let selected = self.voice_settings.values().tts_model_id().cloned();
        let work = cx.background_executor().spawn(async move {
            tts::remove_installation(&root, &descriptor, selected.as_ref())
                .map_err(|error| error.to_string())
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(()) => app
                        .voice_settings
                        .tts_model_manager_mut()
                        .mark_not_installed(&id),
                    Err(error) => app
                        .voice_settings
                        .tts_model_manager_mut()
                        .mark_failed(&id, error),
                };
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn preview_tts_model(&mut self, id: tts::TtsModelId, cx: &mut Context<Self>) {
        if self.dialogue.phase() == dialogue::DialoguePhase::Speaking {
            return;
        }
        let catalog = tts::builtin_model_catalog();
        let Some(descriptor) = catalog.find(&id).cloned() else {
            return;
        };
        let root = self.voice_settings.tts_model_root().join(id.as_str());
        let voice = descriptor
            .resolve_voice_id(
                self.voice_settings
                    .values()
                    .tts_voice_id()
                    .map(tts::TtsVoiceId::as_str),
            )
            .clone();
        cx.background_executor()
            .spawn(async move {
                let Ok(available) = tts::validate_installation(&root, &descriptor) else {
                    return;
                };
                let Ok(mut client) =
                    voice_sherpa::SherpaTtsClient::from_available_model(&available, &voice)
                else {
                    return;
                };
                let Ok(chunk) =
                    dialogue::TtsClient::synthesize(&mut client, "你好，这是当前语音的试听。")
                        .map_err(|_| ())
                else {
                    return;
                };
                crate::app::dialogue::play_tts_preview(&chunk);
            })
            .detach();
    }
}
