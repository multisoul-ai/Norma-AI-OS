use super::{action_button, setting_row, settings_group, text_setting_row, VoiceSettingsInputs};
use crate::app::MultisoulApp;
use crate::settings::voice::state::{VoiceSettingsField, VoiceSettingsState};
use gpui::{div, prelude::*, px, rgba, ClickEvent, Context, Div, Stateful, Window};
use ui::Theme;

pub(super) fn asr_provider_group(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let profile = state.values().azure_asr_profile();
    let azure_active = state.values().asr_provider() == asr::AsrProviderId::CloudAzure;
    let credential_configured = profile.credential_ref().is_some();
    settings_group(theme, "Speech Recognition Provider")
        .child(
            setting_row(
                theme,
                "Active provider",
                "Local Sherpa remains the default; Azure is activated only after validation.",
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(
                        action_button(
                            theme,
                            if azure_active {
                                "Use Local Sherpa"
                            } else {
                                "✓ Local Sherpa"
                            },
                            if azure_active {
                                theme.text_secondary
                            } else {
                                theme.accent_green
                            },
                        )
                        .id("settings-asr-provider-local")
                        .debug_selector(|| "settings-asr-provider-local".to_string())
                        .on_click(cx.listener(MultisoulApp::on_voice_settings_activate_local_asr)),
                    )
                    .child(
                        action_button(
                            theme,
                            if azure_active {
                                "✓ Azure Speech"
                            } else if profile.is_ready() {
                                "Activate Azure"
                            } else {
                                "Azure not ready"
                            },
                            if azure_active || profile.is_ready() {
                                theme.accent_blue
                            } else {
                                theme.text_tertiary
                            },
                        )
                        .id("settings-asr-provider-azure")
                        .debug_selector(|| "settings-asr-provider-azure".to_string())
                        .when(
                            profile.is_ready() && !azure_active,
                            |button| {
                                button.on_click(
                                    cx.listener(MultisoulApp::on_voice_settings_activate_azure_asr),
                                )
                            },
                        ),
                    ),
            ),
        )
        .when(profile.is_ready(), |group| {
            group.child(
                setting_row(
                    theme,
                    "Test recognition",
                    "Records three seconds from the selected microphone without changing the active provider.",
                )
                .child(
                    action_button(theme, "Test Azure (3 sec)", theme.accent_blue)
                        .id("settings-asr-azure-test-recognition")
                        .debug_selector(|| {
                            "settings-asr-azure-test-recognition".to_string()
                        })
                        .on_click(
                            cx.listener(MultisoulApp::on_voice_settings_test_azure_recognition),
                        ),
                ),
            )
        })
        .child(text_setting_row(
            theme,
            inputs,
            VoiceSettingsField::AzureRegion,
            "Azure region",
            "Standard public Azure region, for example eastus",
            cx,
        ))
        .child(locale_catalog(theme, state, cx))
        .child(text_setting_row(
            theme,
            inputs,
            VoiceSettingsField::AzureLocale,
            "Recognition language",
            "Choose a supported Azure locale; zh-CN is the default",
            cx,
        ))
        .child(
            setting_row(
                theme,
                "Azure API Key",
                "Stored only in macOS Keychain and never written to Settings or logs",
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(10.0))
                    .child(inputs.input(VoiceSettingsField::AzureApiKey))
                    .when(credential_configured, |row| {
                        row.child(
                            action_button(theme, "Remove", theme.destructive)
                                .id("settings-asr-azure-key-remove")
                                .debug_selector(|| "settings-asr-azure-key-remove".to_string())
                                .on_click(
                                    cx.listener(MultisoulApp::on_voice_settings_remove_azure_key),
                                ),
                        )
                    }),
            ),
        )
        .child(
            setting_row(
                theme,
                "Validate configuration",
                "Checks region and credentials without changing the active provider.",
            )
            .child(
                action_button(
                    theme,
                    if profile.is_ready() {
                        "✓ Azure Ready"
                    } else {
                        "Validate Azure"
                    },
                    if profile.is_ready() {
                        theme.accent_green
                    } else {
                        theme.accent_blue
                    },
                )
                .id("settings-asr-azure-validate")
                .debug_selector(|| "settings-asr-azure-validate".to_string())
                .on_click(cx.listener(MultisoulApp::on_voice_settings_validate_azure_asr)),
            ),
        )
        .when(azure_active, |group| {
            group.child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgba(theme.accent_blue))
                    .child(
                        "Azure Speech is active for Single Voice Command and Continuous Listening.",
                    ),
            )
        })
}

pub(super) fn locale_catalog(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let query = state.azure_locale_query().trim().to_ascii_lowercase();
    let matches = asr::azure_speech_locales()
        .iter()
        .filter(|locale| locale.to_ascii_lowercase().contains(&query))
        .take(8)
        .copied()
        .collect::<Vec<_>>();
    div()
        .id("settings-asr-azure-locale-catalog")
        .debug_selector(|| "settings-asr-azure-locale-catalog".to_string())
        .w_full()
        .flex()
        .flex_wrap()
        .gap(px(7.0))
        .children(matches.into_iter().enumerate().map(|(index, locale)| {
            let selected = locale == state.values().azure_asr_profile().locale();
            action_button(
                theme,
                locale,
                if selected {
                    theme.accent_green
                } else {
                    theme.text_secondary
                },
            )
            .id(("settings-asr-azure-locale", index))
            .debug_selector(move || format!("settings-asr-azure-locale-{index}"))
            .on_click(cx.listener(move |app, _, _, cx| {
                let region = app
                    .voice_settings
                    .values()
                    .azure_asr_profile()
                    .region()
                    .to_string();
                let credential_ref = app
                    .voice_settings
                    .values()
                    .azure_asr_profile()
                    .credential_ref()
                    .map(ToOwned::to_owned);
                app.voice_settings
                    .update_azure_asr_draft(&region, locale, credential_ref);
                app.voice_settings_inputs
                    .sync_from_state(&app.voice_settings, cx);
                app.persist_voice_settings_in_background(cx);
                cx.notify();
            }))
        }))
}

impl MultisoulApp {
    pub(crate) fn on_voice_settings_activate_local_asr(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice.stop_for_model_switch();
        self.dialogue.stop_shell();
        self.voice_settings.activate_local_sherpa_asr();
        self.persist_voice_settings_in_background(cx);
        cx.notify();
    }

    pub(crate) fn on_voice_settings_activate_azure_asr(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.voice_settings.activate_azure_asr() {
            self.voice.stop_for_model_switch();
            self.dialogue.stop_shell();
            self.persist_voice_settings_in_background(cx);
        }
        cx.notify();
    }

    pub(crate) fn on_voice_settings_remove_azure_key(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.clear_azure_asr_credential();
        self.voice_settings_inputs
            .sync_from_state(&self.voice_settings, cx);
        self.persist_voice_settings_in_background(cx);
        cx.notify();
    }

    pub(crate) fn on_voice_settings_validate_azure_asr(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let profile = self.voice_settings.values().azure_asr_profile().clone();
        let validated_profile = profile.clone();
        let model_root = self.voice_settings.model_root().to_path_buf();
        let work = cx.background_executor().spawn(async move {
            let account = profile
                .credential_ref()
                .ok_or_else(|| "add an API Key first".to_string())?;
            let secret = VoiceSettingsState::load_azure_asr_secret(account)?
                .ok_or_else(|| "API Key is missing from Keychain".to_string())?;
            let config = asr::AzureSpeechConfig::new(
                profile.region(),
                profile.locale(),
                asr::AzureSpeechSecret::new(secret),
            )
            .map_err(|error| error.to_string())?;
            asr::AzureSpeechClient::new(config)
                .validate_credentials()
                .map_err(|error| error.to_string())?;
            crate::settings::voice::models::ensure_silero_vad(&model_root)
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                app.voice_settings
                    .record_azure_asr_validation_result(&validated_profile, result)
                    .then(|| app.persist_voice_settings_in_background(cx));
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn on_voice_settings_test_azure_recognition(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let profile = self.voice_settings.values().azure_asr_profile().clone();
        if !profile.is_ready() {
            return;
        }
        let tested_profile = profile.clone();
        #[cfg(not(feature = "e2e-test-support"))]
        let input_device_id = self
            .voice_settings
            .values()
            .input_device_id()
            .map(ToOwned::to_owned);
        self.voice_settings.begin_azure_asr_recognition_test();
        cx.notify();
        let work = cx.background_executor().spawn(async move {
            let account = profile
                .credential_ref()
                .ok_or_else(|| "API Key is not configured".to_string())?;
            let secret = VoiceSettingsState::load_azure_asr_secret(account)?
                .ok_or_else(|| "API Key is missing from Keychain".to_string())?;
            let config = asr::AzureSpeechConfig::new(
                profile.region(),
                profile.locale(),
                asr::AzureSpeechSecret::new(secret),
            )
            .map_err(|error| error.to_string())?;
            #[cfg(feature = "e2e-test-support")]
            {
                run_deterministic_azure_recognition_test(config)
            }
            #[cfg(not(feature = "e2e-test-support"))]
            {
                voice_input::request_microphone_permission().map_err(|error| error.to_string())?;
                let factory = asr::AzureSpeechRecognizerFactory::new(config);
                let (session, _partials) =
                    crate::app::AsrRecordingSession::start(input_device_id, &factory)?;
                std::thread::sleep(std::time::Duration::from_secs(3));
                session.finish()
            }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                app.voice_settings
                    .record_azure_asr_recognition_test_result(&tested_profile, result);
                cx.notify();
            });
        })
        .detach();
    }
}

#[cfg(feature = "e2e-test-support")]
fn run_deterministic_azure_recognition_test(
    config: asr::AzureSpeechConfig,
) -> Result<String, String> {
    use asr::RecognizerFactory;
    let factory = asr::AzureSpeechRecognizerFactory::new(config);
    let recognizer = factory.create().map_err(|error| error.to_string())?;
    let session = asr::RecognitionPipeline::new(factory.capabilities()).start(recognizer);
    // 20 ms × 16,000 samples/s × 2 bytes/sample = 640 bytes.
    let pcm = audio::PcmAudioChunk::new(audio::PcmFormat::Pcm16, 16_000, 1, vec![0; 640])
        .map_err(|error| format!("invalid deterministic Azure PCM: {error:?}"))?;
    session.push_pcm(pcm).map_err(|error| error.to_string())?;
    session
        .finish(asr::InputEndReason::User)
        .map_err(|error| error.to_string())?;
    for event in session.collect_events() {
        match event {
            asr::RecognitionEvent::Final(text) => return Ok(text),
            asr::RecognitionEvent::Failed(error) => return Err(error.to_string()),
            asr::RecognitionEvent::Cancelled => return Err("recognition was cancelled".to_string()),
            asr::RecognitionEvent::Partial(_)
            | asr::RecognitionEvent::InputEnded(_)
            | asr::RecognitionEvent::Empty => {}
        }
    }
    Err("Azure test completed without recognized speech".to_string())
}
