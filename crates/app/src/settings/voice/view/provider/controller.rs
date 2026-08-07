use super::provider_protocol;
use crate::app::MultisoulApp;
use crate::settings::voice::state::VoiceSettingsState;
use gpui::Context;

impl MultisoulApp {
    pub(crate) fn refresh_voice_provider_credentials(&mut self, cx: &mut Context<Self>) {
        let refs = self.voice_settings.credential_refs();
        let minimax_ref = self.voice_settings.minimax_credential_ref_to_load();
        if refs.is_empty() && minimax_ref.is_none() {
            return;
        }
        let work = cx.background_executor().spawn(async move {
            let agent_credentials = refs
                .into_iter()
                .map(|(provider, account)| {
                    let result = VoiceSettingsState::load_credential(&account);
                    (provider, account, result)
                })
                .collect::<Vec<_>>();
            let minimax_credential = minimax_ref.map(|account| {
                let result = VoiceSettingsState::load_credential(&account);
                (account, result)
            });
            (agent_credentials, minimax_credential)
        });
        cx.spawn(async move |this, cx| {
            let (result, minimax_result) = work.await;
            let _ = this.update(cx, |app, cx| {
                let mut should_validate = false;
                for (provider, account, result) in result {
                    should_validate |= app
                        .voice_settings
                        .apply_loaded_credential(provider, &account, result);
                }
                let should_validate_minimax = minimax_result.is_some_and(|(account, result)| {
                    app.voice_settings
                        .apply_loaded_minimax_credential(&account, result)
                });
                if should_validate {
                    app.validate_saved_voice_provider(cx);
                }
                if should_validate_minimax {
                    app.validate_saved_minimax_tts(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn validate_selected_voice_provider(&mut self, cx: &mut Context<Self>) {
        let provider = self.voice_settings.values().agent_provider();
        let profile = self
            .voice_settings
            .values()
            .active_provider_profile()
            .clone();
        let generation = self.voice_settings.begin_provider_discovery();
        let model = profile.model().to_string();
        let work = cx.background_executor().spawn(async move {
            agent::validate_provider_model(agent::ProviderConfig::new(
                provider_protocol(provider),
                profile.base_url().to_string(),
                model.clone(),
                profile.api_key().map(ToOwned::to_owned),
            ))
            .map(|()| model)
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                if !app
                    .voice_settings
                    .accepts_provider_discovery_generation(generation)
                {
                    return;
                }
                match result {
                    Ok(model) => {
                        app.voice_settings
                            .record_manual_provider_model_validated(model);
                        app.complete_voice_setup_if_ready(cx);
                    }
                    Err(error) => app.voice_settings.record_provider_discovery_failure(error),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
