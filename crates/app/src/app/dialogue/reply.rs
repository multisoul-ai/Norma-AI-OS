//! Stub reply synthesis and assistant playback for Plan 01.

use crate::app::voice::playback::{
    default_assistant_audio_sink, default_preview_audio_sink, interrupt_assistant_playback,
    AssistantAudioSink,
};
#[cfg(test)]
use dialogue::DialoguePhase;
#[cfg(test)]
use dialogue::DialogueSession;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::sync::{Arc, MutexGuard};
use std::time::Duration;
#[cfg(test)]
use voice_sherpa::FakeTtsClient;
use voice_sherpa::{SherpaTtsClient, SpeechError};

pub(crate) type TtsClientBox = Box<dyn tts::TtsClient>;

pub(crate) struct PendingTtsActivation {
    pub(crate) selected: SelectedTtsModel,
    pub(crate) previous_provider_id: Option<tts::TtsProviderId>,
}

#[derive(Clone, Eq)]
pub(crate) struct SelectedTtsModel {
    pub(crate) provider_id: tts::TtsProviderId,
    pub(crate) root: std::path::PathBuf,
    pub(crate) model_id: tts::TtsModelId,
    pub(crate) voice_id: tts::TtsVoiceId,
    pub(crate) credential_ref: Option<String>,
    api_key: Option<String>,
}

impl PartialEq for SelectedTtsModel {
    fn eq(&self, other: &Self) -> bool {
        self.provider_id == other.provider_id
            && self.root == other.root
            && self.model_id == other.model_id
            && self.voice_id == other.voice_id
            && self.credential_ref == other.credential_ref
    }
}

impl SelectedTtsModel {
    pub(crate) fn local(
        root: std::path::PathBuf,
        model_id: tts::TtsModelId,
        voice_id: tts::TtsVoiceId,
    ) -> Self {
        Self {
            provider_id: tts::TtsProviderId::parse(tts::LOCAL_TTS_PROVIDER_ID)
                .expect("built-in On-device TTS Provider ID must be valid"),
            root,
            model_id,
            voice_id,
            credential_ref: None,
            api_key: None,
        }
    }

    pub(crate) fn minimax(
        model_id: tts::TtsModelId,
        voice_id: tts::TtsVoiceId,
        credential_ref: String,
        api_key: String,
    ) -> Self {
        Self {
            provider_id: tts::TtsProviderId::parse(tts::MINIMAX_TTS_PROVIDER_ID)
                .expect("built-in MiniMax TTS Provider ID must be valid"),
            root: std::path::PathBuf::new(),
            model_id,
            voice_id,
            credential_ref: Some(credential_ref),
            api_key: Some(api_key),
        }
    }
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

pub(crate) fn selected_tts_model() -> Option<SelectedTtsModel> {
    SELECTED_TTS_MODEL
        .lock()
        .expect("selected TTS model lock should not be poisoned")
        .clone()
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

pub(crate) fn create_tts_client_for_selected(
    selected: &SelectedTtsModel,
) -> Result<TtsClientBox, SpeechError> {
    let registry = tts::builtin_provider_registry();
    let provider = registry
        .provider(selected.provider_id.as_str())
        .ok_or_else(|| {
            SpeechError::ModelNotFound("unsupported built-in TTS Provider".to_string())
        })?;
    if provider.is_remote() {
        let api_key = selected.api_key.clone().ok_or_else(|| {
            SpeechError::ModelNotFound("remote TTS credential is unavailable".to_string())
        })?;
        return tts::create_builtin_remote_tts_client(tts::BuiltinRemoteTtsRequest {
            provider_id: selected.provider_id.clone(),
            model_id: selected.model_id.clone(),
            voice_id: selected.voice_id.clone(),
            credential: api_key,
        })
        .map_err(|error| SpeechError::ModelNotFound(error.to_string()));
    }
    if provider.id().as_str() != tts::LOCAL_TTS_PROVIDER_ID {
        return Err(SpeechError::ModelNotFound(
            "unsupported built-in TTS Provider".to_string(),
        ));
    }
    let catalog = tts::builtin_model_catalog();
    let descriptor = catalog.find(&selected.model_id).ok_or_else(|| {
        SpeechError::ModelNotFound(format!("unknown TTS model {}", selected.model_id.as_str()))
    })?;
    let available =
        tts::validate_installation(&selected.root.join(selected.model_id.as_str()), descriptor)
            .map_err(|error| SpeechError::ModelNotFound(error.to_string()))?;
    let reference = descriptor
        .capabilities()
        .requires_voice_reference()
        .then(|| tts::load_zipvoice_reference(&selected.root))
        .transpose()
        .map_err(|error| SpeechError::ModelNotFound(error.to_string()))?;
    SherpaTtsClient::from_available_model_with_reference(
        &available,
        &selected.voice_id,
        reference.as_ref(),
    )
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
#[cfg(test)]
pub(crate) fn synthesize_reply(
    tts: &mut dyn tts::TtsClient,
    reply_text: &str,
) -> Result<audio::PcmAudioChunk, SpeechError> {
    let mut segments = collect_tts_segments(tts, &[reply_text])
        .map_err(|error| SpeechError::Recognizer(error.to_string()))?;
    merge_collected_chunks(
        segments
            .pop()
            .expect("one reply segment was requested from TTS"),
    )
}

#[cfg(test)]
fn merge_collected_chunks(
    chunks: Vec<audio::PcmAudioChunk>,
) -> Result<audio::PcmAudioChunk, SpeechError> {
    let mut chunks = chunks.into_iter();
    let mut merged = chunks
        .next()
        .ok_or_else(|| SpeechError::Audio("TTS returned no PCM audio".to_string()))?;
    for chunk in chunks {
        if merged.format != chunk.format
            || merged.sample_rate_hz != chunk.sample_rate_hz
            || merged.channels != chunk.channels
        {
            return Err(SpeechError::Audio(
                "TTS PCM chunks use incompatible formats".to_string(),
            ));
        }
        merged.bytes.extend_from_slice(&chunk.bytes);
    }
    Ok(merged)
}

#[derive(Default)]
struct CollectingTtsSinkState {
    chunks: Vec<audio::PcmAudioChunk>,
    terminal: Option<Result<tts::TtsStreamSummary, tts::TtsError>>,
}

#[derive(Default)]
struct CollectingTtsSink {
    state: Mutex<CollectingTtsSinkState>,
    cancelled: Option<Arc<AtomicBool>>,
}

impl CollectingTtsSink {
    fn lock(&self) -> MutexGuard<'_, CollectingTtsSinkState> {
        self.state
            .lock()
            .expect("collecting TTS sink lock should not be poisoned")
    }
}

impl tts::TtsChunkSink for CollectingTtsSink {
    fn send(&self, chunk: audio::PcmAudioChunk) -> bool {
        self.lock().chunks.push(chunk);
        true
    }

    fn finish(&self, result: Result<tts::TtsStreamSummary, tts::TtsError>) {
        self.lock().terminal = Some(result);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled
            .as_ref()
            .is_some_and(|cancelled| cancelled.load(Ordering::Acquire))
    }
}

pub(crate) fn collect_tts_segments(
    client: &mut dyn tts::TtsClient,
    texts: &[&str],
) -> Result<Vec<Vec<audio::PcmAudioChunk>>, tts::TtsError> {
    collect_tts_segments_with_cancellation(client, texts, None)
}

pub(crate) fn collect_tts_segments_with_cancellation(
    client: &mut dyn tts::TtsClient,
    texts: &[&str],
    cancelled: Option<Arc<AtomicBool>>,
) -> Result<Vec<Vec<audio::PcmAudioChunk>>, tts::TtsError> {
    let sink = Arc::new(CollectingTtsSink {
        state: Mutex::new(CollectingTtsSinkState::default()),
        cancelled,
    });
    client.start_stream(sink.clone())?;
    let mut segments = Vec::with_capacity(texts.len());
    for text in texts {
        if let Err(error) = client.push_text(text) {
            client.cancel_stream();
            return Err(error);
        }
        segments.push(std::mem::take(&mut sink.lock().chunks));
    }
    if let Err(error) = client.finish_stream() {
        client.cancel_stream();
        return Err(error);
    }
    let terminal = sink.lock().terminal.take();
    match terminal {
        Some(Ok(_)) => Ok(segments),
        Some(Err(error)) => Err(error),
        None => Err(tts::TtsError::new(
            tts::TtsErrorKind::Protocol,
            "TTS stream completed without a terminal event",
        )),
    }
}

#[cfg(test)]
pub(crate) fn apply_reply_synthesis(
    session: &mut DialogueSession,
    tts: &mut dyn tts::TtsClient,
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

#[cfg(test)]
pub(crate) fn create_tts_client() -> Result<TtsClientBox, SpeechError> {
    let selected = selected_tts_model();
    create_tts_client_for_selection(selected.as_ref())
}

pub(crate) fn create_tts_client_for_selection(
    selected: Option<&SelectedTtsModel>,
) -> Result<TtsClientBox, SpeechError> {
    if let Some(factory) = *TTS_CLIENT_FACTORY
        .lock()
        .expect("tts client factory lock should not be poisoned")
    {
        return factory();
    }
    let selected = selected
        .ok_or_else(|| SpeechError::ModelNotFound("no Local TTS model selected".to_string()))?;
    create_tts_client_for_selected(selected)
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
