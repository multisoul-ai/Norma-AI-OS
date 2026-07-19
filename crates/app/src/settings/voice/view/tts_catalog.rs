use super::{action_button, setting_label, with_alpha};
use crate::app::MultisoulApp;
use crate::settings::voice::TtsModelInstallState;
use gpui::{div, prelude::*, px, rgba, Context, Div, Window};
use ui::Theme;

mod controller;
mod reference;
pub(crate) use controller::synthesize_tts_preview;
use reference::zipvoice_reference_controls;

pub(super) fn tts_catalog_row(
    theme: Theme,
    state: &crate::settings::voice::VoiceSettingsState,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let selected = state.values().tts_model_id();
    let selected_voice = state.values().tts_voice_id();
    let mut list = div().w_full().flex().flex_col().gap(px(8.0));
    for (index, entry) in state.tts_model_manager().entries().iter().enumerate() {
        let descriptor = entry.descriptor().clone();
        let id = descriptor.id().clone();
        let selected_here = selected == Some(&id);
        list = list.child(
            div()
                .id(("settings-tts-model", index))
                .debug_selector(move || format!("settings-tts-model-{index}"))
                .w_full()
                .max_w(px(620.0))
                .flex()
                .flex_col()
                .gap(px(10.0))
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
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(16.0))
                        .child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(4.0))
                                .child(div().text_color(rgba(theme.text_primary)).child(
                                    if descriptor.experimental() {
                                        format!("{} · Experimental", descriptor.display_name())
                                    } else {
                                        descriptor.display_name().to_string()
                                    },
                                ))
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(rgba(theme.text_tertiary))
                                        .child(format!(
                                            "{} · {} MB · {}",
                                            descriptor
                                                .capabilities()
                                                .languages()
                                                .join(" + ")
                                                .to_uppercase(),
                                            descriptor.package().download_bytes() / 1_000_000,
                                            installation_status(entry.installation())
                                        )),
                                ),
                        )
                        .child(model_action(
                            theme,
                            &descriptor,
                            entry.installation(),
                            selected_here,
                            state.zipvoice_reference_ready(),
                            id.clone(),
                            index,
                            cx,
                        )),
                )
                .children(zipvoice_reference_controls(
                    theme,
                    state,
                    &descriptor,
                    entry.installation(),
                    index,
                    window,
                    cx,
                ))
                .children(voice_controls(
                    theme,
                    &descriptor,
                    entry.installation(),
                    selected_here,
                    selected_voice,
                    state.zipvoice_reference_ready(),
                    index,
                    cx,
                )),
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

fn model_action(
    theme: Theme,
    descriptor: &tts::TtsModelDescriptor,
    installation: &TtsModelInstallState,
    selected: bool,
    zipvoice_reference_ready: bool,
    id: tts::TtsModelId,
    index: usize,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    match installation {
        TtsModelInstallState::NotInstalled => div().child(
            action_button(theme, "Install", theme.accent_orange)
                .id(("settings-tts-install", index))
                .debug_selector(move || format!("settings-tts-install-{index}"))
                .on_click(cx.listener(move |app, _, _, cx| app.install_tts_model(id.clone(), cx))),
        ),
        TtsModelInstallState::Installing { downloaded, total } => {
            let cancel_id = id.clone();
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .text_color(rgba(theme.accent_cyan))
                        .child(format!("{}%", percentage(*downloaded, *total))),
                )
                .child(
                    action_button(theme, "Cancel", theme.destructive)
                        .id(("settings-tts-cancel", index))
                        .debug_selector(move || format!("settings-tts-cancel-{index}"))
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.cancel_tts_install(cancel_id.clone(), cx)
                        })),
                )
        }
        TtsModelInstallState::Cancelling { .. } => div()
            .px(px(12.0))
            .py(px(8.0))
            .text_color(rgba(theme.text_tertiary))
            .child("Cancelling…"),
        TtsModelInstallState::Ready if selected => div()
            .px(px(12.0))
            .py(px(8.0))
            .text_color(rgba(theme.accent_cyan))
            .child("Selected"),
        TtsModelInstallState::Ready
            if descriptor.capabilities().requires_voice_reference()
                && !zipvoice_reference_ready =>
        {
            let delete_id = id;
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .px(px(10.0))
                        .py(px(8.0))
                        .text_size(px(12.0))
                        .text_color(rgba(theme.text_tertiary))
                        .child("Reference required"),
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
        TtsModelInstallState::Ready => {
            let select_id = id.clone();
            let delete_id = id;
            div()
                .flex()
                .gap(px(6.0))
                .child(
                    action_button(theme, "Select", theme.accent_cyan)
                        .id(("settings-tts-select", index))
                        .debug_selector(move || format!("settings-tts-select-{index}"))
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.select_tts_model(select_id.clone(), None, cx)
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
                .on_click(cx.listener(move |app, _, _, cx| app.install_tts_model(id.clone(), cx))),
        ),
    }
}

fn voice_controls(
    theme: Theme,
    descriptor: &tts::TtsModelDescriptor,
    installation: &TtsModelInstallState,
    selected_model: bool,
    selected_voice: Option<&tts::TtsVoiceId>,
    zipvoice_reference_ready: bool,
    model_index: usize,
    cx: &mut Context<MultisoulApp>,
) -> Option<Div> {
    if !matches!(installation, TtsModelInstallState::Ready) {
        return None;
    }
    if descriptor.capabilities().requires_voice_reference() && !zipvoice_reference_ready {
        return None;
    }
    if descriptor.voices().len() == 1 {
        let voice = &descriptor.voices()[0];
        let preview_model = descriptor.id().clone();
        let preview_voice = voice.id().clone();
        return Some(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_size(px(12.0))
                .text_color(rgba(theme.text_tertiary))
                .child(format!("Voice: {}", voice.display_name()))
                .child(
                    action_button(theme, "Preview", theme.accent_cyan)
                        .id(("settings-tts-preview", model_index * 100))
                        .debug_selector(move || format!("settings-tts-preview-{model_index}-0"))
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.preview_tts_voice(preview_model.clone(), preview_voice.clone(), cx)
                        })),
                ),
        );
    }
    let mut controls = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(6.0))
        .text_size(px(12.0))
        .text_color(rgba(theme.text_tertiary))
        .child("Voices:");
    for (voice_index, voice) in descriptor.voices().iter().enumerate() {
        let voice_is_selected = selected_model && selected_voice == Some(voice.id());
        let select_model = descriptor.id().clone();
        let select_voice = voice.id().clone();
        let preview_model = descriptor.id().clone();
        let preview_voice = voice.id().clone();
        let control_id = model_index * 100 + voice_index;
        controls = controls
            .child(voice.display_name().to_string())
            .child(
                action_button(
                    theme,
                    if voice_is_selected { "Selected" } else { "Use" },
                    if voice_is_selected {
                        theme.accent_orange
                    } else {
                        theme.accent_cyan
                    },
                )
                .id(("settings-tts-voice", control_id))
                .debug_selector(move || format!("settings-tts-voice-{model_index}-{voice_index}"))
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.select_tts_model(select_model.clone(), Some(select_voice.clone()), cx)
                })),
            )
            .child(
                action_button(theme, "Preview", theme.accent_cyan)
                    .id(("settings-tts-preview", control_id))
                    .debug_selector(move || {
                        format!("settings-tts-preview-{model_index}-{voice_index}")
                    })
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.preview_tts_voice(preview_model.clone(), preview_voice.clone(), cx)
                    })),
            );
    }
    Some(controls)
}

fn percentage(downloaded: u64, total: u64) -> u64 {
    if total == 0 {
        0
    } else {
        downloaded.saturating_mul(100) / total
    }
}

fn installation_status(installation: &TtsModelInstallState) -> &'static str {
    match installation {
        TtsModelInstallState::NotInstalled => "Not installed",
        TtsModelInstallState::Installing { .. } => "Installing",
        TtsModelInstallState::Cancelling { .. } => "Cancelling",
        TtsModelInstallState::Ready => "Ready",
        TtsModelInstallState::Failed(_) => "Installation failed",
    }
}
