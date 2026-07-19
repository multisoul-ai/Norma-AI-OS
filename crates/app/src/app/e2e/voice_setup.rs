use crate::app::MultisoulApp;
use gpui::Context;

impl MultisoulApp {
    pub fn active_asr_provider_for_e2e(&self) -> &'static str {
        self.voice_settings.values().asr_provider().as_str()
    }

    pub fn azure_asr_ready_for_e2e(&self) -> bool {
        self.voice_settings.values().azure_asr_profile().is_ready()
    }

    pub fn voice_settings_status_for_e2e(&self) -> Option<(String, bool)> {
        self.voice_settings
            .status()
            .map(|(message, is_error)| (message.to_string(), is_error))
    }

    pub fn assume_voice_ready_for_e2e(&mut self) {
        self.voice_settings.assume_legacy_ready_for_e2e();
        self.voice_settings.assume_agent_provider_ready_for_e2e();
    }

    pub fn install_voice_pcm_for_e2e(&mut self, chunks: Vec<Vec<u8>>) {
        let chunks = chunks
            .into_iter()
            .map(|bytes| {
                audio::PcmAudioChunk::new(audio::PcmFormat::Pcm16, 16_000, 1, bytes)
                    .expect("e2e PCM must be valid 16kHz mono PCM16")
            })
            .collect();
        voice_input::install_test_pcm_stream(chunks);
    }

    pub fn voice_has_active_speech_session_for_e2e(&self) -> bool {
        self.voice.has_speech_session()
    }

    pub fn pending_voice_transcript_for_e2e(&self) -> Option<String> {
        self.dialogue.pending_transcript().map(ToOwned::to_owned)
    }

    pub fn reset_voice_runtime_for_e2e(&mut self) {
        self.voice.stop_for_model_switch();
        self.dialogue.stop_shell();
    }

    pub fn set_voice_result_delay_for_e2e(&mut self, delay_ms: Option<u64>) {
        self.voice
            .set_result_application_delay(delay_ms.map(std::time::Duration::from_millis));
    }

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
