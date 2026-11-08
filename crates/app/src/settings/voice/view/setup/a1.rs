use super::components::{compact_setup_row, setup_action_button, SetupButtonTone};
use crate::{
    app::MultisoulApp,
    settings::{
        typography::{SettingsText, TextRole},
        voice::{VoiceRequirementState, VoiceSettingsState, VoiceSetupDestination},
    },
};
use gpui::{div, prelude::*, px, rgba, ClickEvent, Context, Div, KeyDownEvent, Stateful, Window};
use ui::Theme;

pub(in crate::settings::voice::view) fn a1_detail(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &super::VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let active = state.values().asr_provider() == asr::AsrProviderId::LocalA1;
    let allowed = state
        .values()
        .desktop_mode()
        .supports_local_a1(state.values().interaction_mode());
    let ready = state.a1_requirement() == VoiceRequirementState::Ready;
    let checking = matches!(
        state.a1_requirement(),
        VoiceRequirementState::Installing { .. }
    );
    let mut action = div().flex().gap(px(8.0));
    if checking {
        action = action.child(ui::i18n::text("voice.a1_checking"));
    } else {
        action = action.child(setup_action_button(
            theme,
            "voice-a1-validate",
            ui::i18n::text("voice.a1_validate"),
            SetupButtonTone::Default,
            cx,
            cx.listener(MultisoulApp::on_voice_validate_a1),
        ));
        if ready && allowed && !active {
            action = action.child(setup_action_button(
                theme,
                "voice-a1-activate",
                ui::i18n::text("voice.a1_activate"),
                SetupButtonTone::Accent,
                cx,
                cx.listener(MultisoulApp::on_voice_activate_a1),
            ));
        } else if ready && active {
            action = action.child(ui::i18n::text("voice.a1_selected"));
        }
    }
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .when_some(inputs.a1_panel.clone(), |detail, panel| detail.child(panel))
        .child(compact_setup_row(
            theme,
            ui::i18n::text("voice.a1_readiness"),
            ui::i18n::text(if allowed && ready && !active {
                "voice.a1_ready_to_activate"
            } else if allowed {
                "voice.a1_description"
            } else {
                "voice.a1_single_only"
            }),
            action,
        ))
        .when_some(state.a1_validation_error(), |detail, error| {
            detail.child(super::super::capabilities::a1_preparation_failure(error))
        })
        .when(!ready && !checking, |detail| {
            detail.child(ui::i18n::text("voice.a1_setup_required"))
        })
}

/// 主界面只保留一行状态。点击展开「更多设置」，完整安装与取消留在那里。
pub(in crate::settings::voice::view) fn a1_status_row(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let active = state.values().asr_provider() == asr::AsrProviderId::LocalA1;
    let allowed = state
        .values()
        .desktop_mode()
        .supports_local_a1(state.values().interaction_mode());
    let ready = state.a1_requirement() == VoiceRequirementState::Ready;
    let checking = matches!(
        state.a1_requirement(),
        VoiceRequirementState::Installing { .. }
    );
    let hint = if checking {
        "voice.a1_checking"
    } else if !allowed {
        "voice.a1_single_only"
    } else if ready && active {
        "voice.a1_selected"
    } else if ready {
        "voice.a1_ready_to_activate"
    } else {
        "voice.a1_setup_required"
    };
    let hover = super::super::with_alpha(theme.text_primary, 0x10);
    let pressed = super::super::with_alpha(theme.text_primary, 0x1c);
    div()
        .id("voice-a1-status-row")
        .debug_selector(|| "voice-a1-status-row".to_string())
        .w_full()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .gap(px(12.0))
        .px(px(12.0))
        .py(px(10.0))
        .rounded(px(8.0))
        .cursor_pointer()
        .border_1()
        .border_color(rgba(0x00000000))
        .hover(move |row| row.bg(rgba(hover)))
        .active(move |row| row.bg(rgba(pressed)))
        .tab_index(0)
        .focus(move |row| row.border_color(rgba(theme.accent_blue)))
        .on_click(cx.listener(MultisoulApp::on_voice_settings_show_advanced))
        .on_key_down(cx.listener(|app, event: &KeyDownEvent, _, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                app.voice_settings.expand_advanced();
                cx.notify();
                cx.stop_propagation();
            }
        }))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .settings_text(TextRole::RowTitle, theme)
                        .child(ui::i18n::text("voice.a1_name")),
                )
                .child(
                    div()
                        .settings_text(TextRole::Helper, theme)
                        .child(ui::i18n::text(hint)),
                ),
        )
        .child(
            div()
                .flex_none()
                .settings_text(TextRole::Helper, theme)
                .text_color(rgba(theme.accent_blue))
                .child(ui::i18n::text("voice.more_settings")),
        )
}

impl MultisoulApp {
    pub(crate) fn on_voice_setup_show_a1(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings
            .open_setup_destination(VoiceSetupDestination::AsrA1);
        cx.notify();
    }
    pub(crate) fn on_voice_validate_a1(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_a1_requirement(cx);
    }
    pub(crate) fn refresh_a1_requirement(&mut self, cx: &mut Context<Self>) {
        if matches!(
            self.voice_settings.a1_requirement(),
            VoiceRequirementState::Installing { .. }
        ) {
            return;
        }
        self.prepare_or_validate_a1(false, cx);
    }
    pub(crate) fn prepare_or_validate_a1(&mut self, activate: bool, cx: &mut Context<Self>) {
        if activate {
            self.invalidate_single_voice_commands();
        }
        let prepare = activate
            || (self.voice_settings.values().asr_provider() == asr::AsrProviderId::LocalA1
                && self
                    .voice_settings
                    .values()
                    .desktop_mode()
                    .supports_local_a1(self.voice_settings.values().interaction_mode()));
        let generation = self.voice_settings.begin_a1_validation();
        let cancellation = self.voice_settings.a1_preparation_cancellation();
        let root = self.voice_settings.model_root().to_owned();
        let work = cx.background_executor().spawn(async move {
            if cancellation.is_cancelled() {
                return Err("A1 preparation cancelled".to_string());
            }
            crate::settings::voice::a1_install::ensure_runtime(&root, &cancellation)
                .and_then(|config| {
                    if prepare {
                        asr::A1RecognizerFactory::new(config)
                            .prepare_with_cancellation(cancellation)
                    } else {
                        config.validate()
                    }
                })
                .map_err(|e| e.to_string())
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            if let Err(error) = &result {
                tracing::warn!(%error, "A1 preparation failed");
            }
            let _ = this.update(cx, |app, cx| {
                if app.voice_settings.finish_a1_validation(generation, result) {
                    if activate && app.voice_settings.activate_local_a1() {
                        app.voice.stop_for_model_switch();
                        app.stop_continuous_listening(cx);
                        app.persist_voice_settings_in_background(cx);
                    }
                    app.complete_voice_setup_if_ready(cx);
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
    pub(crate) fn on_voice_activate_a1(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.voice_settings.a1_requirement() == VoiceRequirementState::Ready
            && self
                .voice_settings
                .values()
                .desktop_mode()
                .supports_local_a1(self.voice_settings.values().interaction_mode())
        {
            self.prepare_or_validate_a1(true, cx);
        }
        cx.notify();
    }
}
