//! Stub reply synthesis and assistant playback for Plan 01.

use crate::app::voice::playback::{
    default_assistant_audio_sink, interrupt_assistant_playback, AssistantAudioSink,
};
use dialogue::{DialoguePhase, DialogueSession};
use std::sync::Mutex;
#[cfg(test)]
use voice_sherpa::FakeTtsClient;
use voice_sherpa::{SherpaTtsClient, SpeechError};

pub(crate) type TtsClientBox = Box<dyn dialogue::TtsClient>;

pub(crate) fn default_tts_client() -> Result<TtsClientBox, SpeechError> {
    SherpaTtsClient::from_env_or_default().map(|client| Box::new(client) as TtsClientBox)
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

    pub(crate) fn play_chunk(&mut self, chunk: &audio::PcmAudioChunk) {
        self.sink.write_chunk(chunk);
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

pub(crate) fn finish_reply_playback(session: &mut DialogueSession, keep_listening: bool) {
    session.on_playback_finished_or_interrupted(keep_listening);
}

pub(crate) fn apply_reply_synthesis(
    session: &mut DialogueSession,
    tts: &mut dyn dialogue::TtsClient,
    playback: &mut ReplyPlayback,
    reply_text: &str,
) -> Result<(), SpeechError> {
    if session.phase() != DialoguePhase::Thinking {
        return Err(SpeechError::Recognizer(format!(
            "reply synthesis requires Thinking, got {:?}",
            session.phase()
        )));
    }
    let chunk = synthesize_reply(tts, reply_text)?;
    session
        .on_reply_ready()
        .map_err(|error| SpeechError::Recognizer(error.to_string()))?;
    playback.play_chunk(&chunk);
    Ok(())
}

static TTS_CLIENT_FACTORY: Mutex<Option<fn() -> Result<TtsClientBox, SpeechError>>> =
    Mutex::new(None);
static AUDIO_SINK_FACTORY: Mutex<Option<fn() -> Box<dyn AssistantAudioSink>>> = Mutex::new(None);

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

#[cfg(test)]
pub(crate) fn set_tts_client_factory_for_tests(
    factory: Option<fn() -> Result<TtsClientBox, SpeechError>>,
) {
    *TTS_CLIENT_FACTORY
        .lock()
        .expect("tts client factory lock should not be poisoned") = factory;
}

#[cfg(test)]
pub(crate) fn set_audio_sink_factory_for_tests(
    factory: Option<fn() -> Box<dyn AssistantAudioSink>>,
) {
    *AUDIO_SINK_FACTORY
        .lock()
        .expect("audio sink factory lock should not be poisoned") = factory;
}
