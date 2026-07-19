use super::provider_protocol;
use crate::app::MultisoulApp;
use crate::settings::voice::state::VoiceSettingsState;
use gpui::Context;

impl MultisoulApp {
    pub(crate) fn refresh_voice_provider_credentials(&mut self, cx: &mut Context<Self>) {
        let refs = self.voice_settings.credential_refs();
        if refs.is_empty() {
            return;
        }
        let work = cx.background_executor().spawn(async move {
            refs.into_iter()
                .map(|(provider, account)| {
                    let result = VoiceSettingsState::load_credential(&account);
                    (provider, account, result)
                })
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                let mut should_validate = false;
                for (provider, account, result) in result {
                    should_validate |= app
                        .voice_settings
                        .apply_loaded_credential(provider, &account, result);
                }
                if should_validate {
                    app.validate_saved_voice_provider(cx);
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
