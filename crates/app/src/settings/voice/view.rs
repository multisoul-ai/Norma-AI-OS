mod diagnostics;
use self::asr_provider::asr_provider_group;
use self::doubao::{desktop_mode_picker, doubao_settings};
pub(crate) use self::editable::VoiceSettingsInputs;
use self::input_device::input_device_row;
use self::interaction::interaction_group;
use self::model_catalog::model_catalog_row;
use self::provider::{agent_provider_advanced_rows, agent_provider_group};
use self::ready::{voice_ready_overview, voice_status_overview};
use self::setup::voice_setup;
use self::spoken_reply::spoken_reply_group;
use self::status::status_message;
use super::state::{VoiceSettingsState, VoiceSetupDestination};
use crate::app::MultisoulApp;
use diagnostics::trace_row;
use gpui::{div, prelude::*, px, rgba, ClickEvent, Context, Div, FontWeight, Window};
use ui::Theme;

mod asr_provider;
mod components;
mod doubao;
mod editable;
mod input_device;
mod interaction;
mod model_catalog;
mod options;
mod provider;
mod ready;
mod setup;
mod spoken_reply;
mod status;
mod tts_catalog;
mod tts_provider;

pub(super) use components::{action_button, text_setting_row};
pub(crate) use options::VoiceSettingsView;

pub(crate) fn voice_settings(
    view: VoiceSettingsView<'_>,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let VoiceSettingsView {
        theme,
        state,
        inputs,
        content_scroll,
        setup_scroll_target,
        voice_shortcut_label,
    } = view;
    let page = div()
        .w_full()
        .max_w(px(920.0))
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(20.0))
        .child(desktop_mode_picker(theme, state, cx))
        .child(voice_status_overview(
            theme,
            state,
            voice_shortcut_label,
            cx,
        ));
    if state.values().desktop_mode() == super::DesktopVoiceMode::Doubao {
        return page
            .child(doubao_settings(
                theme,
                state,
                inputs,
                voice_shortcut_label,
                window,
                cx,
            ))
            .children(status_message(theme, state));
    }
    if !matches!(state.readiness(), super::VoiceEntryState::Ready) || state.reviewing_setup() {
        return page
            .child(voice_setup(
                theme,
                state,
                inputs,
                content_scroll,
                setup_scroll_target,
                window,
                cx,
            ))
            .children(status_message(theme, state));
    }
    page.child(voice_ready_overview(theme, voice_shortcut_label, cx))
        .child(interaction_group(theme, state, voice_shortcut_label, cx))
        .child(
            settings_group(theme, ui::i18n::text("voice.input"))
                .child(input_device_row(theme, state, cx)),
        )
        .child(spoken_reply_group(theme, state, inputs, cx))
        .child(asr_provider_group(theme, state, inputs, cx))
        .child(advanced_group(theme, state, inputs, cx))
        .children(status_message(theme, state))
}

impl MultisoulApp {
    pub(crate) fn on_voice_review_setup(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.set_reviewing_setup(true);
        cx.notify();
    }

    pub(crate) fn on_voice_finish_review_setup(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.set_reviewing_setup(false);
        cx.notify();
    }

    pub(crate) fn on_voice_manage_asr_models(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.set_reviewing_setup(true);
        self.voice_settings
            .open_setup_destination(VoiceSetupDestination::Overview);
        self.settings_viewport
            .content_scroll
            .set_offset(gpui::Point::default());
        cx.notify();
    }

    pub(crate) fn on_voice_manage_tts_models(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.set_reviewing_setup(true);
        self.voice_settings
            .open_setup_destination(VoiceSetupDestination::SpokenReplies);
        self.voice_settings.open_tts_picker();
        self.voice_settings_inputs
            .sync_from_state(&self.voice_settings, cx);
        self.settings_viewport.pending_voice_setup_scroll =
            Some(VoiceSetupDestination::SpokenReplies);
        cx.notify();
    }

    pub(crate) fn on_voice_manage_provider(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.set_reviewing_setup(true);
        self.voice_settings
            .open_setup_destination(VoiceSetupDestination::AgentProvider);
        self.settings_viewport.pending_voice_setup_scroll =
            Some(VoiceSetupDestination::AgentProvider);
        cx.notify();
    }

    pub(crate) fn on_voice_setup_toggle_models(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.toggle_setup_models_expanded();
        cx.notify();
    }

    pub(crate) fn on_voice_setup_request_microphone(
        &mut self,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.can_open_microphone_privacy_settings() {
            self.on_voice_open_microphone_privacy_settings(event, window, cx);
            return;
        }
        self.request_voice_microphone_access(cx);
    }

    pub(crate) fn on_voice_settings_refresh_input_devices(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.refresh_input_devices();
        cx.notify();
    }

    pub(crate) fn on_voice_settings_toggle_input_device_picker(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.toggle_input_device_picker();
        cx.notify();
    }

    pub(crate) fn on_voice_settings_use_default_input_device(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stop_doubao_voice(cx);
        self.voice_settings.select_input_device(None);
        self.voice_settings.close_input_device_picker();
        self.persist_voice_settings_in_background(cx);
        cx.notify();
    }

    pub(crate) fn on_voice_settings_toggle_trace(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.toggle_trace();
        self.persist_voice_settings_in_background(cx);
        cx.notify();
    }

    pub(crate) fn on_voice_settings_toggle_agent_task_announcements(
        &mut self,
        _: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.toggle_agent_task_announcements();
        if !self
            .voice_settings
            .values()
            .agent_task_announcements_enabled()
        {
            self.cancel_agent_announcements();
        }
        self.persist_voice_settings_in_background(cx);
        cx.notify();
    }

    pub(crate) fn on_voice_settings_toggle_window_inset(
        &mut self,
        _: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.toggle_voice_window_inset();
        self.sync_voice_window_inset_enabled();
        self.persist_voice_settings_in_background(cx);
        cx.notify();
    }

    pub(crate) fn on_voice_settings_toggle_advanced(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.toggle_advanced_expanded();
        cx.notify();
    }

    pub(crate) fn persist_voice_settings_in_background(&mut self, cx: &mut Context<Self>) {
        let tts_activation_batch = self.pending_tts_activation.is_some();
        let writes = self.voice_settings.take_pending_writes();
        if writes.is_empty() {
            return;
        }
        let work = cx.background_executor().spawn(async move {
            if tts_activation_batch {
                crate::settings::voice::PendingVoiceSettingWrite::execute_settings_atomically(
                    writes,
                )?;
            } else {
                for write in writes {
                    write.execute().map_err(|error| error.to_string())?;
                }
            }
            Ok::<(), String>(())
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                let credential_succeeded = result.is_ok();
                let completion = app.voice_settings.record_persistence_result(result);
                let start_minimax_preview =
                    if app.pending_minimax_preview_after_validation.is_some()
                        && completion.minimax_credential
                    {
                        let generation = app.pending_minimax_preview_after_validation.take();
                        credential_succeeded
                            .then_some(generation)
                            .flatten()
                            .filter(|generation| {
                                app.voice_settings
                                    .accepts_minimax_connection_generation(*generation)
                            })
                    } else {
                        None
                    };
                if completion.minimax_clear && credential_succeeded {
                    app.cancel_agent_announcements();
                    app.tts_preview.cancel();
                    app.invalidate_tts_for_configuration_change(cx);
                    app.dialogue.selected_tts_model = None;
                }
                if let Some(activation) = app.pending_tts_activation.take() {
                    let selected = &activation.selected;
                    let picker_accepts_activation =
                        app.voice_settings.tts_picker_accepts_activation(
                            &selected.provider_id,
                            &selected.model_id,
                            &selected.voice_id,
                        );
                    if credential_succeeded && tts_activation_batch && picker_accepts_activation {
                        app.complete_tts_activation(activation, cx);
                    } else {
                        app.voice_settings
                            .restore_tts_activation(activation.rollback);
                        app.voice_settings
                            .fail_tts_picker_apply(ui::i18n::LocalizedText::new(
                            "voice.could_not_save_the_selected_voice_the_previous_voice_is_still",
                        ));
                    }
                }
                app.persist_voice_settings_in_background(cx);
                if completion.agent_credential && credential_succeeded {
                    app.validate_saved_voice_provider(cx);
                }
                if !tts_activation_batch
                    && (completion.minimax_credential || completion.minimax_validation)
                {
                    app.configure_active_minimax_tts_if_ready(cx);
                }
                if start_minimax_preview.is_some() {
                    app.start_minimax_tts_preview(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }
}

fn advanced_group(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let expanded = state.advanced_expanded();
    settings_group(theme, ui::i18n::text("voice.advanced_settings"))
        .child(
            setting_row(
                theme,
                ui::i18n::text("voice.recognition_models_provider_diagnostics"),
                ui::i18n::text(
                    "voice.manage_the_asr_catalog_connection_details_and_troubleshooting",
                ),
            )
            .child(
                action_button(
                    theme,
                    if expanded {
                        ui::i18n::text("voice.hide")
                    } else {
                        ui::i18n::text("voice.show")
                    },
                    theme.accent_cyan,
                )
                .id("settings-voice-advanced-toggle")
                .debug_selector(|| "settings-voice-advanced-toggle".to_string())
                .on_click(cx.listener(MultisoulApp::on_voice_settings_toggle_advanced)),
            ),
        )
        .when(expanded, |group| {
            group
                .child(model_catalog_row(theme, state, cx))
                .child(agent_provider_group(theme, state, inputs, cx))
                .child(agent_provider_advanced_rows(theme, state, inputs, cx))
                .child(trace_row(theme, state, cx))
        })
}

pub(super) fn settings_group(theme: Theme, title: &'static str) -> Div {
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .p(px(20.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.text_primary, 0x18)))
        .bg(rgba(theme.panel_high))
        .child(
            div()
                .text_size(px(16.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgba(theme.text_primary))
                .child(title),
        )
}

pub(super) fn setting_row(theme: Theme, title: &'static str, hint: &'static str) -> Div {
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .items_start()
        .gap(px(10.0))
        .py(px(6.0))
        .child(setting_label(theme, title, hint))
}

pub(super) fn setting_label(theme: Theme, title: &'static str, hint: &'static str) -> Div {
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .text_size(px(14.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgba(theme.text_primary))
                .child(title),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(rgba(theme.text_tertiary))
                .child(hint),
        )
}

pub(super) fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
