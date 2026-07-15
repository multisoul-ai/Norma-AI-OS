//! Stub reply synthesis and assistant playback for Plan 01.

use crate::app::voice::playback::{
    default_assistant_audio_sink, default_preview_audio_sink, interrupt_assistant_playback,
    AssistantAudioSink,
};
#[cfg(test)]
use dialogue::DialoguePhase;
#[cfg(test)]
use dialogue::DialogueSession;
use std::sync::Mutex;
use std::time::Duration;
#[cfg(test)]
use voice_sherpa::FakeTtsClient;
use voice_sherpa::{SherpaTtsClient, SpeechError};

pub(crate) type TtsClientBox = Box<dyn dialogue::TtsClient>;

#[derive(Clone)]
pub(crate) struct SelectedTtsModel {
    pub(crate) root: std::path::PathBuf,
    pub(crate) model_id: tts::TtsModelId,
    pub(crate) voice_id: tts::TtsVoiceId,
}

static SELECTED_TTS_MODEL: Mutex<Option<SelectedTtsModel>> = Mutex::new(None);

pub(crate) fn configure_selected_tts_model(model: Option<SelectedTtsModel>) {
    *SELECTED_TTS_MODEL
        .lock()
        .expect("selected TTS model lock should not be poisoned") = model;
}

pub(crate) fn selected_tts_model_configured() -> bool {
    SELECTED_TTS_MODEL
        .lock()
        .expect("selected TTS model lock should not be poisoned")
        .is_some()
}

pub(crate) fn selected_tts_model_matches(
    model_id: Option<&tts::TtsModelId>,
    voice_id: Option<&tts::TtsVoiceId>,
) -> bool {
    let (Some(model_id), Some(voice_id)) = (model_id, voice_id) else {
        return false;
    };
    SELECTED_TTS_MODEL
        .lock()
        .expect("selected TTS model lock should not be poisoned")
        .as_ref()
        .is_some_and(|selected| &selected.model_id == model_id && &selected.voice_id == voice_id)
}

pub(crate) fn default_tts_client() -> Result<TtsClientBox, SpeechError> {
    let selected = SELECTED_TTS_MODEL
        .lock()
        .expect("selected TTS model lock should not be poisoned")
        .clone()
        .ok_or_else(|| SpeechError::ModelNotFound("no Local TTS model selected".to_string()))?;
    let catalog = tts::builtin_model_catalog();
    let descriptor = catalog.find(&selected.model_id).ok_or_else(|| {
        SpeechError::ModelNotFound(format!("unknown TTS model {}", selected.model_id.as_str()))
    })?;
    let available =
        tts::validate_installation(&selected.root.join(selected.model_id.as_str()), descriptor)
            .map_err(|error| SpeechError::ModelNotFound(error.to_string()))?;
    SherpaTtsClient::from_available_model(&available, &selected.voice_id)
        .map(|client| Box::new(client) as TtsClientBox)
}

#[cfg(test)]
pub(crate) fn fake_tts_client_for_tests() -> TtsClientBox {
    Box::new(FakeTtsClient::silent_pcm16_16k())
}

pub(crate) struct ReplyPlayback {
    sink: Box<dyn AssistantAudioSink>,
}

impl ReplyPlayback {
    pub(crate) fn new(sink: Box<dyn AssistantAudioSink>) -> Self {
        Self { sink }
    }

    #[cfg(test)]
    pub(crate) fn play_chunk(
        &mut self,
        chunk: &audio::PcmAudioChunk,
    ) -> Result<Duration, SpeechError> {
        self.sink.write_chunk(chunk).map_err(SpeechError::Audio)
    }

    pub(crate) fn play_chunk_with_completion(
        &mut self,
        chunk: &audio::PcmAudioChunk,
        playback_started: async_channel::Sender<()>,
        completion: async_channel::Sender<()>,
    ) -> Result<Duration, SpeechError> {
        self.sink
            .write_chunk_with_completion(chunk, playback_started, completion)
            .map_err(SpeechError::Audio)
    }

    pub(crate) fn interrupt(&mut self) {
        interrupt_assistant_playback(self.sink.as_mut());
    }
}
pub(crate) fn synthesize_reply(
    tts: &mut dyn dialogue::TtsClient,
    reply_text: &str,
) -> Result<audio::PcmAudioChunk, SpeechError> {
    tts.synthesize(reply_text)
        .map_err(|error| SpeechError::Recognizer(error.to_string()))
}

#[cfg(test)]
pub(crate) fn apply_reply_synthesis(
    session: &mut DialogueSession,
    tts: &mut dyn dialogue::TtsClient,
    playback: &mut ReplyPlayback,
    reply_text: &str,
) -> Result<Duration, SpeechError> {
    if session.phase() != DialoguePhase::Thinking {
        return Err(SpeechError::Recognizer(format!(
            "reply synthesis requires Thinking, got {:?}",
            session.phase()
        )));
    }
    let chunk = synthesize_reply(tts, reply_text)?;
    schedule_reply_chunk(session, playback, &chunk)
}

#[cfg(test)]
pub(crate) fn schedule_reply_chunk(
    session: &mut DialogueSession,
    playback: &mut ReplyPlayback,
    chunk: &audio::PcmAudioChunk,
) -> Result<Duration, SpeechError> {
    session
        .on_reply_ready()
        .map_err(|error| SpeechError::Recognizer(error.to_string()))?;
    playback.play_chunk(chunk)
}

type TtsClientFactory = fn() -> Result<TtsClientBox, SpeechError>;
type AssistantAudioSinkFactory = fn() -> Box<dyn AssistantAudioSink>;

static TTS_CLIENT_FACTORY: Mutex<Option<TtsClientFactory>> = Mutex::new(None);
static AUDIO_SINK_FACTORY: Mutex<Option<AssistantAudioSinkFactory>> = Mutex::new(None);

pub(crate) fn create_tts_client() -> Result<TtsClientBox, SpeechError> {
    if let Some(factory) = *TTS_CLIENT_FACTORY
        .lock()
        .expect("tts client factory lock should not be poisoned")
    {
        return factory();
    }
    default_tts_client()
}

pub(crate) fn create_assistant_audio_sink() -> Box<dyn AssistantAudioSink> {
    if let Some(factory) = *AUDIO_SINK_FACTORY
        .lock()
        .expect("audio sink factory lock should not be poisoned")
    {
        return factory();
    }
    default_assistant_audio_sink()
}

pub(crate) fn create_preview_audio_sink() -> Box<dyn AssistantAudioSink> {
    default_preview_audio_sink()
}

#[cfg(test)]
pub(crate) fn set_tts_client_factory_for_tests(factory: Option<TtsClientFactory>) {
    *TTS_CLIENT_FACTORY
        .lock()
        .expect("tts client factory lock should not be poisoned") = factory;
}

#[cfg(test)]
pub(crate) fn set_audio_sink_factory_for_tests(factory: Option<AssistantAudioSinkFactory>) {
    *AUDIO_SINK_FACTORY
        .lock()
        .expect("audio sink factory lock should not be poisoned") = factory;
}
