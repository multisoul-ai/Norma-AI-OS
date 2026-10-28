use super::components::{compact_setup_row, setup_action_button, SetupButtonTone};
use crate::{
    app::MultisoulApp,
    settings::voice::{VoiceRequirementState, VoiceSettingsState, VoiceSetupDestination},
};
use gpui::{div, prelude::*, px, ClickEvent, Context, Div, Window};
use ui::Theme;

pub(super) fn a1_detail(
    theme: Theme,
    state: &VoiceSettingsState,
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
        .child(compact_setup_row(
            theme,
            ui::i18n::text("voice.a1_name"),
            ui::i18n::text(if allowed {
                "voice.a1_description"
            } else {
                "voice.a1_single_only"
            }),
            action,
        ))
        .when_some(state.a1_validation_error(), |detail, _error| {
            detail.child(ui::i18n::text("voice.a1_prepare_failed"))
        })
        .when(!ready && !checking, |detail| {
            detail.child(ui::i18n::text("voice.a1_setup_required"))
        })
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
    fn prepare_or_validate_a1(&mut self, activate: bool, cx: &mut Context<Self>) {
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
            asr::A1Config::from_model_root(&root)
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
