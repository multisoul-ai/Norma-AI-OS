use super::super::with_alpha;
use super::components::{compact_setup_row, setup_action_button, SetupButtonTone};
use crate::app::MultisoulApp;
use crate::settings::voice::{AsrModelInstallState, VoiceSettingsState};
use asr::{AsrModelDescriptor, AsrModelId};
use gpui::{div, prelude::*, px, relative, rgba, Context, Div, Stateful};
use ui::Theme;

pub(super) fn alternative_models_detail(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let expanded = state.setup_models_expanded();
    let current = selected_model_summary(state);
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(compact_setup_row(
            theme,
            ui::i18n::text("voice.recognition_model_3"),
            current,
            setup_action_button(
                theme,
                "voice-setup-alternative-models-toggle",
                if expanded {
                    ui::i18n::text("voice.hide_models")
                } else {
                    ui::i18n::text("voice.choose_other_models")
                },
                SetupButtonTone::Accent,
                cx,
                cx.listener(MultisoulApp::on_voice_setup_toggle_models),
            ),
        ))
        .when(expanded, |section| {
            section.child(compact_model_catalog(theme, state, cx))
        })
}

fn selected_model_summary(state: &VoiceSettingsState) -> String {
    state
        .values()
        .asr_model_id()
        .and_then(|id| asr::builtin_model_catalog().find(id).cloned())
        .map(|model| {
            format!(
                "{} · {}",
                model.display_name(),
                model.capabilities().languages().join(" + ").to_uppercase()
            )
        })
        .unwrap_or_else(|| ui::i18n::text("voice.no_on_device_model_selected").to_string())
}

fn compact_model_catalog(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let selected = state.values().asr_model_id();
    let local_active = state.values().asr_provider() == asr::AsrProviderId::LocalSherpa;
    div()
        .id("voice-setup-alternative-models")
        .debug_selector(|| "voice-setup-alternative-models".to_string())
        .w_full()
        .flex()
        .flex_col()
        .border_t_1()
        .border_color(rgba(with_alpha(theme.text_primary, 0x20)))
        .children(
            state
                .model_manager()
                .entries()
                .iter()
                .enumerate()
                .map(|(index, entry)| {
                    let descriptor = entry.descriptor().clone();
                    let is_selected = selected == Some(descriptor.id());
                    compact_model_row(
                        theme,
                        index,
                        descriptor,
                        entry.installation(),
                        is_selected,
                        local_active,
                        cx,
                    )
                }),
        )
        .child(
            div()
                .pt(px(8.0))
                .text_size(px(11.0))
                .text_color(rgba(theme.text_tertiary))
                .child(ui::i18n::text(
                    "voice.downloads_continue_when_voice_setup_is_closed",
                )),
        )
}

fn compact_model_row(
    theme: Theme,
    index: usize,
    descriptor: AsrModelDescriptor,
    installation: &AsrModelInstallState,
    is_selected: bool,
    local_active: bool,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let id = descriptor.id().clone();
    let action = model_action_control(
        theme,
        index,
        &id,
        installation,
        is_selected,
        ModelActionLabels {
            selected: if local_active {
                ui::i18n::text("voice.active")
            } else {
                ui::i18n::text("voice.selected")
            },
            install: ui::i18n::text("voice.download"),
            use_model: ui::i18n::text("voice.use"),
        },
        cx,
    );
    let status = compact_install_status(installation, descriptor.artifact().archive_bytes());
    div()
        .id(("settings-asr-model", index))
        .debug_selector(move || format!("settings-asr-model-{index}"))
        .w_full()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(14.0))
        .px(px(4.0))
        .py(px(11.0))
        .border_b_1()
        .border_color(rgba(with_alpha(theme.text_primary, 0x18)))
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(rgba(theme.text_primary))
                        .child(descriptor.display_name().to_string()),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgba(theme.text_tertiary))
                        .child(format!(
                            "{} · {status}",
                            descriptor
                                .capabilities()
                                .languages()
                                .join(" + ")
                                .to_uppercase()
                        )),
                ),
        )
        .child(action)
}

pub(in crate::settings::voice::view) struct ModelActionLabels {
    pub(in crate::settings::voice::view) selected: &'static str,
    pub(in crate::settings::voice::view) install: &'static str,
    pub(in crate::settings::voice::view) use_model: &'static str,
}

pub(in crate::settings::voice::view) fn model_action_control(
    theme: Theme,
    index: usize,
    id: &AsrModelId,
    installation: &AsrModelInstallState,
    is_selected: bool,
    labels: ModelActionLabels,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let ModelActionLabels {
        selected: selected_label,
        install: install_label,
        use_model: use_label,
    } = labels;
    match installation {
        AsrModelInstallState::NotInstalled => {
            let id = id.clone();
            div().child(setup_action_button(
                theme,
                format!("settings-asr-install-{index}"),
                install_label,
                SetupButtonTone::Accent,
                cx,
                cx.listener(move |app, _, _, cx| app.install_asr_model(id.clone(), cx)),
            ))
        }
        AsrModelInstallState::Installing { downloaded, total } => {
            let percent = if *total == 0 {
                0
            } else {
                downloaded.saturating_mul(100) / total
            };
            let id = id.clone();
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .w(px(96.0))
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgba(theme.accent_cyan))
                                .child(ui::i18n::format(
                                    "voice.downloading_percent",
                                    &[("percent", (percent).to_string())],
                                )),
                        )
                        .child(
                            div()
                                .w_full()
                                .h(px(4.0))
                                .rounded(px(2.0))
                                .bg(rgba(with_alpha(theme.node_stroke, 0x48)))
                                .child(
                                    div()
                                        .h_full()
                                        .w(relative(percent as f32 / 100.0))
                                        .rounded(px(2.0))
                                        .bg(rgba(theme.accent_blue)),
                                ),
                        ),
                )
                .child(setup_action_button(
                    theme,
                    format!("settings-asr-cancel-{index}"),
                    ui::i18n::text("common.cancel"),
                    SetupButtonTone::Destructive,
                    cx,
                    cx.listener(move |app, _, _, cx| app.cancel_asr_install(id.clone(), cx)),
                ))
        }
        AsrModelInstallState::Ready if is_selected => div()
            .px(px(10.0))
            .py(px(7.0))
            .text_size(px(12.0))
            .text_color(rgba(theme.accent_green))
            .child(selected_label),
        AsrModelInstallState::Ready => {
            let select_id = id.clone();
            let delete_id = id.clone();
            div()
                .flex()
                .gap(px(6.0))
                .child(setup_action_button(
                    theme,
                    format!("settings-asr-select-{index}"),
                    use_label,
                    SetupButtonTone::Accent,
                    cx,
                    cx.listener(move |app, _, _, cx| {
                        app.select_ready_asr_model(select_id.clone(), cx)
                    }),
                ))
                .child(setup_action_button(
                    theme,
                    format!("settings-asr-delete-{index}"),
                    ui::i18n::text("common.delete"),
                    SetupButtonTone::Destructive,
                    cx,
                    cx.listener(move |app, _, _, cx| app.delete_asr_model(delete_id.clone(), cx)),
                ))
        }
        AsrModelInstallState::Failed(_) => {
            let id = id.clone();
            div().child(setup_action_button(
                theme,
                format!("settings-asr-retry-{index}"),
                ui::i18n::text("common.retry"),
                SetupButtonTone::Destructive,
                cx,
                cx.listener(move |app, _, _, cx| app.install_asr_model(id.clone(), cx)),
            ))
        }
    }
}

fn compact_install_status(state: &AsrModelInstallState, archive_bytes: u64) -> String {
    let size = if archive_bytes >= 1_000_000_000 {
        format!("{:.2} GB", archive_bytes as f64 / 1_000_000_000.0)
    } else {
        format!("{} MB", archive_bytes / 1_000_000)
    };
    match state {
        AsrModelInstallState::NotInstalled => size,
        AsrModelInstallState::Installing { downloaded, total } => ui::i18n::format(
            "voice.of_mb",
            &[
                ("arg0", (downloaded / 1_000_000).to_string()),
                ("arg1", (total / 1_000_000).to_string()),
            ],
        ),
        AsrModelInstallState::Ready => {
            ui::i18n::format("voice.ready_size", &[("size", (size).to_string())])
        }
        AsrModelInstallState::Failed(_) => ui::i18n::text("voice.download_failed").to_string(),
    }
}
