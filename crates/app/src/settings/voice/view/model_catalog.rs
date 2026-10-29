use super::setup::models::{model_action_control, ModelActionLabels};
use super::{setting_label, with_alpha};
use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use crate::settings::voice::{
    AsrModelInstallState, AsrModelManagerState, VoiceRequirementState, VoiceSettingsState,
};
use asr::{AsrModelDescriptor, AsrModelId};
use gpui::{div, prelude::*, px, rgba, Context, Div};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use ui::Theme;

enum ModelInstallEvent {
    Progress { downloaded: u64, total: u64 },
    Finished(ModelInstallOutcome),
}

enum ModelInstallOutcome {
    Ready,
    Cancelled,
    Failed(String),
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
        let action = model_action_control(
            theme,
            index,
            &id,
            entry.installation(),
            is_selected,
            ModelActionLabels {
                selected: ui::i18n::text("voice.selected"),
                install: ui::i18n::text("common.install"),
                use_model: ui::i18n::text("voice.select"),
            },
            cx,
        );
        let status =
            install_state_label(entry.installation(), descriptor.artifact().archive_bytes());
        let recommendation = if descriptor.recommended() {
            ui::i18n::text("voice.recommended")
        } else {
            ui::i18n::text("voice.recommendation_pending")
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
                                .settings_text(TextRole::Helper, theme)
                                .child(ui::i18n::format(
                                    "voice.streaming_status_recommendation",
                                    &[
                                        (
                                            "arg0",
                                            (descriptor
                                                .capabilities()
                                                .languages()
                                                .join(" + ")
                                                .to_uppercase())
                                            .to_string(),
                                        ),
                                        ("status", (status).to_string()),
                                        ("recommendation", (recommendation).to_string()),
                                    ],
                                )),
                        )
                        .when_some(failure_message(entry.installation()), |element, message| {
                            element.child(
                                div()
                                    .settings_font(TextRole::Helper)
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
            ui::i18n::text("voice.recognition_model"),
            ui::i18n::text("voice.installed_and_validated_models_are_shared_by_both_voice_modes"),
        ))
        .child(list)
}

fn install_state_label(state: &AsrModelInstallState, archive_bytes: u64) -> String {
    match state {
        AsrModelInstallState::NotInstalled => ui::i18n::format(
            "voice.not_installed_mb",
            &[("arg0", (archive_bytes / 1_000_000).to_string())],
        ),
        AsrModelInstallState::Installing { downloaded, total } => ui::i18n::format(
            "voice.installing_mb",
            &[
                ("arg0", (downloaded / 1_000_000).to_string()),
                ("arg1", (total / 1_000_000).to_string()),
            ],
        ),
        AsrModelInstallState::Ready => ui::i18n::format(
            "voice.ready_mb",
            &[("arg0", (archive_bytes / 1_000_000).to_string())],
        ),
        AsrModelInstallState::Failed(_) => ui::i18n::text("voice.installation_failed").to_string(),
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
        self.refresh_vad_requirement(cx);
        if self.voice_settings.values().asr_provider() == asr::AsrProviderId::LocalA1
            && self
                .voice_settings
                .values()
                .desktop_mode()
                .supports_local_a1(self.voice_settings.values().interaction_mode())
        {
            self.refresh_a1_requirement(cx);
        }
        let generation = self.voice_settings.begin_asr_model_scan();
        let model_root = self.voice_settings.model_root().to_path_buf();
        let work = cx.background_executor().spawn(async move {
            AsrModelManagerState::scan(&asr::builtin_model_catalog(), &model_root)
        });
        cx.spawn(async move |this, cx| {
            let mut manager = work.await;
            let _ = this.update(cx, |app, cx| {
                if !app.voice_settings.accepts_asr_model_scan(generation) {
                    return;
                }
                // 磁盘扫描不能覆盖本会话仍在进行的下载及最新进度。
                for id in app.asr_download_cancellations.keys() {
                    if let Some(AsrModelInstallState::Installing { downloaded, total }) =
                        app.voice_settings.model_manager().installation(id)
                    {
                        manager.update_progress(id, *downloaded, *total);
                    }
                }
                app.voice_settings.replace_model_manager(manager);
                app.complete_voice_setup_if_ready(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn refresh_vad_requirement(&mut self, cx: &mut Context<Self>) {
        let generation = self.voice_settings.begin_vad_validation();
        let model_root = self.voice_settings.model_root().to_path_buf();
        let work = cx.background_executor().spawn(async move {
            match asr::validate_silero_vad_installation(&model_root) {
                Ok(_) => VoiceRequirementState::Ready,
                Err(asr::AsrInstallationError::MissingRequiredFile(_)) => {
                    VoiceRequirementState::Missing
                }
                Err(error) => {
                    tracing::warn!(
                        %error,
                        "shared VAD integrity validation failed during background refresh"
                    );
                    VoiceRequirementState::Failed
                }
            }
        });
        cx.spawn(async move |this, cx| {
            let requirement = work.await;
            let _ = this.update(cx, |app, cx| {
                if !app
                    .voice_settings
                    .finish_vad_validation(generation, requirement)
                {
                    return;
                }
                app.complete_voice_setup_if_ready(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn install_asr_model(&mut self, id: AsrModelId, cx: &mut Context<Self>) {
        if matches!(
            self.voice_settings.model_manager().installation(&id),
            Some(AsrModelInstallState::Ready) | Some(AsrModelInstallState::Installing { .. })
        ) {
            return;
        }
        let catalog = asr::builtin_model_catalog();
        let Some(descriptor) = catalog.find(&id).cloned() else {
            return;
        };
        let model_root = self.voice_settings.model_root().to_path_buf();
        self.voice_settings.begin_asr_model_scan();
        self.voice_settings
            .model_manager_mut()
            .begin_install(&id, descriptor.artifact().archive_bytes());
        let cancelled = Arc::new(AtomicBool::new(false));
        self.asr_download_cancellations
            .insert(id.clone(), Arc::clone(&cancelled));
        let (event_tx, event_rx) = async_channel::bounded::<ModelInstallEvent>(32);
        cx.background_executor()
            .spawn(async move {
                let result = install_or_revalidate(
                    &descriptor,
                    &model_root,
                    &cancelled,
                    |downloaded, total| {
                        let _ = event_tx
                            .send_blocking(ModelInstallEvent::Progress { downloaded, total });
                    },
                );
                let outcome = match result {
                    Ok(()) if cancelled.load(Ordering::Acquire) => ModelInstallOutcome::Cancelled,
                    Ok(()) => ModelInstallOutcome::Ready,
                    Err(_) if cancelled.load(Ordering::Acquire) => ModelInstallOutcome::Cancelled,
                    Err(error) => ModelInstallOutcome::Failed(error),
                };
                let _ = event_tx.send_blocking(ModelInstallEvent::Finished(outcome));
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
                        ModelInstallEvent::Finished(ModelInstallOutcome::Ready) => {
                            app.asr_download_cancellations.remove(&id);
                            app.voice_settings.complete_asr_model_install(id.clone());
                            app.refresh_vad_requirement(cx);
                            app.complete_voice_setup_if_ready(cx);
                            app.persist_voice_settings_in_background(cx);
                            app.broadcast_voice_settings_changed(true, cx);
                        }
                        ModelInstallEvent::Finished(ModelInstallOutcome::Cancelled) => {
                            app.asr_download_cancellations.remove(&id);
                            app.refresh_asr_model_states(cx);
                        }
                        ModelInstallEvent::Finished(ModelInstallOutcome::Failed(error)) => {
                            app.fail_asr_model_install(&id, error);
                        }
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

    pub(crate) fn cancel_asr_install(&mut self, id: AsrModelId, cx: &mut Context<Self>) {
        let Some(cancelled) = self.asr_download_cancellations.get(&id) else {
            return;
        };
        cancelled.store(true, Ordering::Release);
        cx.notify();
    }

    fn fail_asr_model_install(&mut self, id: &AsrModelId, error: String) {
        self.voice_settings.begin_asr_model_scan();
        self.asr_download_cancellations.remove(id);
        self.voice_settings
            .model_manager_mut()
            .mark_failed(id, error);
    }

    pub(crate) fn select_ready_asr_model(&mut self, id: AsrModelId, cx: &mut Context<Self>) {
        let generation = self.voice_settings.begin_asr_model_selection();
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
                if !app
                    .voice_settings
                    .accepts_asr_model_selection_generation(generation)
                {
                    return;
                }
                match result {
                    Ok(id) => {
                        if app
                            .voice_settings
                            .should_restart_for_asr_model_selection(&id)
                        {
                            app.stop_continuous_listening(cx);
                            app.voice.stop_for_model_switch();
                        }
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
                    Ok(()) => {
                        app.voice_settings
                            .model_manager_mut()
                            .mark_not_installed(&id);
                        app.broadcast_voice_settings_changed(true, cx);
                    }
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

#[cfg(test)]
#[path = "../../../../tests/unit/asr_model_scan_race_tests.rs"]
mod scan_race_tests;

fn install_or_revalidate(
    descriptor: &AsrModelDescriptor,
    model_root: &std::path::Path,
    cancelled: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<(), String> {
    #[cfg(feature = "e2e-test-support")]
    if crate::settings::voice::models::e2e_fake_asr_install_enabled() {
        progress(
            descriptor.artifact().archive_bytes(),
            descriptor.artifact().archive_bytes(),
        );
        return Ok(());
    }
    let install_root = model_root.join(descriptor.id().as_str());
    let available = match asr::validate_installation(&install_root, descriptor) {
        Ok(available) => available,
        Err(_) => {
            if install_root.exists() {
                std::fs::remove_dir_all(&install_root).map_err(|error| {
                    format!("could not clear invalid model installation: {error}")
                })?;
            }
            let archive = crate::settings::voice::models::download_model_archive_cancellable(
                descriptor,
                &model_root.join(".downloads"),
                cancelled,
                &mut progress,
            )?;
            if cancelled.load(Ordering::Acquire) {
                let _ = std::fs::remove_file(archive);
                return Err("model installation cancelled".to_string());
            }
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
        let vad_file = crate::settings::voice::models::download_silero_vad_cancellable(
            &model_root.join(".downloads"),
            cancelled,
            &mut progress,
        )?;
        if cancelled.load(Ordering::Acquire) {
            let _ = std::fs::remove_file(vad_file);
            return Err("model installation cancelled".to_string());
        }
        asr::install_silero_vad_file(&vad_file, model_root).map_err(|error| error.to_string())?;
        let _ = std::fs::remove_file(vad_file);
    }
    voice_sherpa::validate_model_load(&available).map_err(|error| error.to_string())
}
