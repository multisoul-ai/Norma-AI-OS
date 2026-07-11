use super::{action_button, setting_label, with_alpha};
use crate::app::MultisoulApp;
use crate::settings::voice::{AsrModelInstallState, AsrModelManagerState, VoiceSettingsState};
use asr::{AsrModelDescriptor, AsrModelId};
use gpui::{div, prelude::*, px, rgba, Context, Div};
use ui::Theme;

enum ModelInstallEvent {
    Progress { downloaded: u64, total: u64 },
    Finished(Result<(), String>),
}

pub(super) fn model_catalog_row(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let selected = state.values().asr_model_id();
    let mut list = div().w_full().flex().flex_col().gap(px(8.0));
    for (index, entry) in state.model_manager().entries().iter().enumerate() {
        let descriptor = entry.descriptor().clone();
        let id = descriptor.id().clone();
        let is_selected = selected == Some(&id);
        let action = match entry.installation() {
            AsrModelInstallState::NotInstalled => div().child(
                action_button(theme, "Install", theme.accent_orange)
                    .id(("settings-asr-install", index))
                    .debug_selector(move || format!("settings-asr-install-{index}"))
                    .on_click(
                        cx.listener(move |app, _, _, cx| app.install_asr_model(id.clone(), cx)),
                    ),
            ),
            AsrModelInstallState::Installing { downloaded, total } => {
                let percent = if *total == 0 {
                    0
                } else {
                    downloaded.saturating_mul(100) / total
                };
                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .text_color(rgba(theme.accent_cyan))
                    .child(format!("{percent}%"))
            }
            AsrModelInstallState::Ready if is_selected => div()
                .px(px(12.0))
                .py(px(8.0))
                .text_color(rgba(theme.accent_cyan))
                .child("Selected"),
            AsrModelInstallState::Ready => {
                let select_id = id.clone();
                let delete_id = id.clone();
                div()
                    .flex()
                    .flex_row()
                    .gap(px(6.0))
                    .child(
                        action_button(theme, "Select", theme.accent_cyan)
                            .id(("settings-asr-select", index))
                            .debug_selector(move || format!("settings-asr-select-{index}"))
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.select_ready_asr_model(select_id.clone(), cx)
                            })),
                    )
                    .child(
                        action_button(theme, "Delete", theme.destructive)
                            .id(("settings-asr-delete", index))
                            .debug_selector(move || format!("settings-asr-delete-{index}"))
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.delete_asr_model(delete_id.clone(), cx)
                            })),
                    )
            }
            AsrModelInstallState::Failed(_) => div().child(
                action_button(theme, "Retry", theme.destructive)
                    .id(("settings-asr-retry", index))
                    .debug_selector(move || format!("settings-asr-retry-{index}"))
                    .on_click(
                        cx.listener(move |app, _, _, cx| app.install_asr_model(id.clone(), cx)),
                    ),
            ),
        };
        let status =
            install_state_label(entry.installation(), descriptor.artifact().archive_bytes());
        let recommendation = if descriptor.recommended() {
            "Recommended"
        } else {
            "Recommendation pending"
        };
        list = list.child(
            div()
                .id(("settings-asr-model", index))
                .debug_selector(move || format!("settings-asr-model-{index}"))
                .w_full()
                .max_w(px(620.0))
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(px(16.0))
                .px(px(14.0))
                .py(px(12.0))
                .rounded(px(10.0))
                .border_1()
                .border_color(rgba(with_alpha(
                    if is_selected {
                        theme.accent_orange
                    } else {
                        theme.edge_highlight
                    },
                    if is_selected { 0x90 } else { 0x45 },
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
                                    "{} · Streaming · {status} · {recommendation}",
                                    descriptor
                                        .capabilities()
                                        .languages()
                                        .join(" + ")
                                        .to_uppercase()
                                )),
                        )
                        .when_some(failure_message(entry.installation()), |element, message| {
                            element.child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgba(theme.destructive))
                                    .child(message),
                            )
                        }),
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
            "Recognition Model",
            "Installed and validated models are shared by both voice modes",
        ))
        .child(list)
}

fn install_state_label(state: &AsrModelInstallState, archive_bytes: u64) -> String {
    match state {
        AsrModelInstallState::NotInstalled => {
            format!("Not installed · {} MB", archive_bytes / 1_000_000)
        }
        AsrModelInstallState::Installing { downloaded, total } => format!(
            "Installing · {} / {} MB",
            downloaded / 1_000_000,
            total / 1_000_000
        ),
        AsrModelInstallState::Ready => format!("Ready · {} MB", archive_bytes / 1_000_000),
        AsrModelInstallState::Failed(_) => "Installation failed".to_string(),
    }
}

fn failure_message(state: &AsrModelInstallState) -> Option<String> {
    match state {
        AsrModelInstallState::Failed(message) => Some(message.clone()),
        _ => None,
    }
}

impl MultisoulApp {
    pub(crate) fn refresh_asr_model_states(&mut self, cx: &mut Context<Self>) {
        let model_root = self.voice_settings.model_root().to_path_buf();
        let work = cx.background_executor().spawn(async move {
            AsrModelManagerState::scan(&asr::builtin_model_catalog(), &model_root)
        });
        cx.spawn(async move |this, cx| {
            let manager = work.await;
            let _ = this.update(cx, |app, cx| {
                app.voice_settings.replace_model_manager(manager);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn install_asr_model(&mut self, id: AsrModelId, cx: &mut Context<Self>) {
        let catalog = asr::builtin_model_catalog();
        let Some(descriptor) = catalog.find(&id).cloned() else {
            return;
        };
        let model_root = self.voice_settings.model_root().to_path_buf();
        self.voice_settings
            .model_manager_mut()
            .begin_install(&id, descriptor.artifact().archive_bytes());
        let (event_tx, event_rx) = async_channel::bounded::<ModelInstallEvent>(32);
        cx.background_executor()
            .spawn(async move {
                let result =
                    install_or_revalidate(&descriptor, &model_root, |downloaded, total| {
                        let _ =
                            event_tx.try_send(ModelInstallEvent::Progress { downloaded, total });
                    });
                let _ = event_tx.send_blocking(ModelInstallEvent::Finished(result));
            })
            .detach();
        cx.spawn(async move |this, cx| {
            while let Ok(event) = event_rx.recv().await {
                let finished = matches!(event, ModelInstallEvent::Finished(_));
                let _ = this.update(cx, |app, cx| {
                    match event {
                        ModelInstallEvent::Progress { downloaded, total } => app
                            .voice_settings
                            .model_manager_mut()
                            .update_progress(&id, downloaded, total),
                        ModelInstallEvent::Finished(Ok(())) => {
                            app.stop_continuous_listening(cx);
                            app.voice.stop_for_model_switch();
                            app.voice_settings.model_manager_mut().mark_ready(&id);
                            app.voice_settings.select_asr_model(Some(id.clone()));
                            app.persist_voice_settings_in_background(cx);
                        }
                        ModelInstallEvent::Finished(Err(error)) => app
                            .voice_settings
                            .model_manager_mut()
                            .mark_failed(&id, error),
                    }
                    cx.notify();
                });
                if finished {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn select_ready_asr_model(&mut self, id: AsrModelId, cx: &mut Context<Self>) {
        self.stop_continuous_listening(cx);
        self.voice.stop_for_model_switch();
        let model_root = self.voice_settings.model_root().to_path_buf();
        let requested_id = id.clone();
        let work = cx.background_executor().spawn(async move {
            let catalog = asr::builtin_model_catalog();
            let descriptor = catalog
                .find(&id)
                .ok_or_else(|| "unknown ASR model".to_string())?;
            let available = asr::validate_installation(&model_root.join(id.as_str()), descriptor)
                .map_err(|error| error.to_string())?;
            voice_sherpa::validate_model_load(&available).map_err(|error| error.to_string())?;
            Ok::<AsrModelId, String>(id)
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(id) => {
                        app.voice_settings.select_asr_model(Some(id));
                        app.persist_voice_settings_in_background(cx);
                    }
                    Err(error) => {
                        tracing::warn!(%error, "ASR model switch failed; keeping previous selection");
                        app.voice_settings
                            .model_manager_mut()
                            .mark_failed(&requested_id, error);
                    }
                }
                cx.notify();
            });
        }).detach();
    }

    pub(crate) fn delete_asr_model(&mut self, id: AsrModelId, cx: &mut Context<Self>) {
        let model_root = self.voice_settings.model_root().to_path_buf();
        let selected = self.voice_settings.values().asr_model_id().cloned();
        let worker_id = id.clone();
        let work = cx.background_executor().spawn(async move {
            let catalog = asr::builtin_model_catalog();
            let descriptor = catalog
                .find(&worker_id)
                .ok_or_else(|| "unknown ASR model".to_string())?;
            asr::remove_installation(
                &model_root.join(worker_id.as_str()),
                descriptor,
                selected.as_ref(),
            )
            .map_err(|error| error.to_string())
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(()) => app
                        .voice_settings
                        .model_manager_mut()
                        .mark_not_installed(&id),
                    Err(error) => app
                        .voice_settings
                        .model_manager_mut()
                        .mark_failed(&id, error),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

fn install_or_revalidate(
    descriptor: &AsrModelDescriptor,
    model_root: &std::path::Path,
    mut progress: impl FnMut(u64, u64),
) -> Result<(), String> {
    let install_root = model_root.join(descriptor.id().as_str());
    let available = match asr::validate_installation(&install_root, descriptor) {
        Ok(available) => available,
        Err(_) => {
            if install_root.exists() {
                std::fs::remove_dir_all(&install_root).map_err(|error| {
                    format!("could not clear invalid model installation: {error}")
                })?;
            }
            let archive = crate::settings::voice::models::download_model_archive(
                descriptor,
                &model_root.join(".downloads"),
                &mut progress,
            )?;
            let available = crate::settings::voice::models::install_downloaded_archive(
                descriptor, &archive, model_root,
            )?;
            let _ = std::fs::remove_file(archive);
            available
        }
    };
    if asr::validate_silero_vad_installation(model_root).is_err() {
        let vad_root = model_root.join(asr::SILERO_VAD_DIRECTORY);
        if vad_root.exists() {
            std::fs::remove_dir_all(&vad_root)
                .map_err(|error| format!("could not clear invalid VAD installation: {error}"))?;
        }
        let vad_file = crate::settings::voice::models::download_silero_vad(
            &model_root.join(".downloads"),
            &mut progress,
        )?;
        asr::install_silero_vad_file(&vad_file, model_root).map_err(|error| error.to_string())?;
        let _ = std::fs::remove_file(vad_file);
    }
    voice_sherpa::validate_model_load(&available).map_err(|error| error.to_string())
}
