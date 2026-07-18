use crate::app::MultisoulApp;
use gpui::Context;

impl MultisoulApp {
    pub fn reset_voice_setup_for_e2e(&mut self) {
        self.voice_settings.reset_voice_setup_for_e2e();
    }

    pub fn set_voice_setup_install_fakes_for_e2e(&mut self, enabled: bool) {
        crate::settings::voice::set_e2e_fake_asr_install(enabled);
        crate::settings::voice::tts_models::set_e2e_fake_tts_transport(enabled, false);
    }

    pub fn set_voice_setup_requirements_for_e2e(
        &mut self,
        microphone_ready: bool,
        provider_ready: bool,
    ) {
        self.voice_settings
            .set_setup_requirements_for_e2e(microphone_ready, provider_ready);
    }

    pub fn start_required_voice_model_installs_for_e2e(&mut self, cx: &mut Context<Self>) {
        let asr = asr::AsrModelId::parse(asr::ZIPFORMER_BILINGUAL_MODEL_ID)
            .expect("built-in Zipformer model id must be valid");
        let tts = tts::TtsModelId::parse(tts::MATCHA_ZH_EN_MODEL_ID)
            .expect("built-in Matcha model id must be valid");
        self.install_asr_model(asr, cx);
        self.install_tts_model(tts, cx);
    }
}
