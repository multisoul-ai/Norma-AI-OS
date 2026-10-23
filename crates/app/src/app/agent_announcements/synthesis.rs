use super::cache::CachedAnnouncementSegmentLookup;
use super::{AgentAnnouncement, AgentAnnouncementOutcome};
use crate::app::dialogue::collect_tts_segments;
use audio::{PcmAudioChunk, PcmFormat};
use std::sync::Arc;

const AGENT_TERMINAL_PAUSE_MILLIS: usize = 250;
const PCM16_BYTES_PER_SAMPLE: usize = 2;

pub(super) struct SynthesizedAgentTerminalAnnouncement {
    pub(super) name: Arc<PcmAudioChunk>,
    pub(super) outcome: Arc<PcmAudioChunk>,
    pub(super) pcm: PcmAudioChunk,
}

impl AgentAnnouncement {
    pub(super) fn terminal(
        node_id: canvas::NodeId,
        agent_name: &str,
        outcome: AgentAnnouncementOutcome,
    ) -> Self {
        let outcome_word = outcome.spoken_word().to_string();
        Self {
            node_id,
            text: format!("{agent_name}, {outcome_word}"),
            agent_name: agent_name.to_string(),
            outcome_word,
            report: false,
        }
    }

    #[cfg(test)]
    pub(super) fn new(node_id: canvas::NodeId, text: impl Into<String>) -> Self {
        let text = text.into();
        let (agent_name, outcome_word) = text
            .rsplit_once(", ")
            .expect("Agent announcement test text must contain `, `");
        Self {
            node_id,
            agent_name: agent_name.to_string(),
            outcome_word: outcome_word.to_string(),
            text,
            report: false,
        }
    }
}

#[cfg(test)]
pub(super) fn synthesize_agent_terminal_announcement(
    tts: &mut dyn tts::TtsClient,
    agent_name: &str,
    outcome_word: &str,
) -> Result<PcmAudioChunk, voice_sherpa::SpeechError> {
    synthesize_agent_terminal_announcement_with_cached_segments(
        tts,
        agent_name,
        outcome_word,
        CachedAnnouncementSegmentLookup::default(),
    )
    .map(|synthesized| synthesized.pcm)
}

pub(super) fn synthesize_agent_terminal_announcement_with_cached_segments(
    tts: &mut dyn tts::TtsClient,
    agent_name: &str,
    outcome_word: &str,
    cached: CachedAnnouncementSegmentLookup,
) -> Result<SynthesizedAgentTerminalAnnouncement, voice_sherpa::SpeechError> {
    let name_missing = cached.name.is_none();
    let outcome_missing = cached.outcome.is_none();
    let mut requested_texts =
        Vec::with_capacity(usize::from(name_missing) + usize::from(outcome_missing));
    if name_missing {
        requested_texts.push(agent_name);
    }
    if outcome_missing {
        requested_texts.push(outcome_word);
    }
    let mut generated_segments = if requested_texts.is_empty() {
        Vec::new().into_iter()
    } else {
        collect_tts_segments(tts, &requested_texts)
            .map_err(|error| voice_sherpa::SpeechError::Recognizer(error.to_string()))?
            .into_iter()
    };
    let name = match cached.name {
        Some(name) => name,
        None => Arc::new(merge_segment_chunks(
            generated_segments
                .next()
                .expect("missing name segment must be requested from TTS"),
        )?),
    };
    let outcome = match cached.outcome {
        Some(outcome) => outcome,
        None => Arc::new(merge_segment_chunks(
            generated_segments
                .next()
                .expect("missing outcome segment must be requested from TTS"),
        )?),
    };
    assemble_cached_agent_terminal_announcement(CachedAnnouncementSegmentLookup {
        name: Some(name),
        outcome: Some(outcome),
    })
}

/// 大脑的播报：整段文字一次合成，不进名字和结果的分段缓存。
pub(super) fn synthesize_report_text(
    tts: &mut dyn tts::TtsClient,
    text: &str,
) -> Result<SynthesizedAgentTerminalAnnouncement, voice_sherpa::SpeechError> {
    let segments = collect_tts_segments(tts, &[text])
        .map_err(|error| voice_sherpa::SpeechError::Recognizer(error.to_string()))?
        .into_iter()
        .next()
        .unwrap_or_default();
    let pcm = merge_segment_chunks(segments)?;
    let shared = Arc::new(pcm.clone());
    Ok(SynthesizedAgentTerminalAnnouncement {
        name: Arc::clone(&shared),
        outcome: shared,
        pcm,
    })
}

pub(super) fn assemble_cached_agent_terminal_announcement(
    cached: CachedAnnouncementSegmentLookup,
) -> Result<SynthesizedAgentTerminalAnnouncement, voice_sherpa::SpeechError> {
    let name = cached.name.ok_or_else(|| {
        voice_sherpa::SpeechError::Audio(
            "cached Agent announcement is missing the name PCM segment".to_string(),
        )
    })?;
    let outcome = cached.outcome.ok_or_else(|| {
        voice_sherpa::SpeechError::Audio(
            "cached Agent announcement is missing the outcome PCM segment".to_string(),
        )
    })?;
    let pcm = join_with_agent_terminal_pause((*name).clone(), (*outcome).clone())?;
    Ok(SynthesizedAgentTerminalAnnouncement { name, outcome, pcm })
}

fn merge_segment_chunks(
    chunks: Vec<PcmAudioChunk>,
) -> Result<PcmAudioChunk, voice_sherpa::SpeechError> {
    let mut chunks = chunks.into_iter();
    let mut merged = chunks.next().ok_or_else(|| {
        voice_sherpa::SpeechError::Audio("TTS segment returned no PCM audio".to_string())
    })?;
    for chunk in chunks {
        if merged.format != chunk.format
            || merged.sample_rate_hz != chunk.sample_rate_hz
            || merged.channels != chunk.channels
        {
            return Err(voice_sherpa::SpeechError::Audio(
                "TTS segment PCM chunks use incompatible formats".to_string(),
            ));
        }
        merged.bytes.extend_from_slice(&chunk.bytes);
    }
    Ok(merged)
}

fn join_with_agent_terminal_pause(
    name_pcm: PcmAudioChunk,
    outcome_pcm: PcmAudioChunk,
) -> Result<PcmAudioChunk, voice_sherpa::SpeechError> {
    if name_pcm.format != outcome_pcm.format
        || name_pcm.sample_rate_hz != outcome_pcm.sample_rate_hz
        || name_pcm.channels != outcome_pcm.channels
    {
        return Err(voice_sherpa::SpeechError::Audio(
            "Agent announcement PCM segments use incompatible formats".to_string(),
        ));
    }
    let bytes_per_sample = match name_pcm.format {
        PcmFormat::Pcm16 => PCM16_BYTES_PER_SAMPLE,
    };
    let pause_frames = (name_pcm.sample_rate_hz as usize)
        .checked_mul(AGENT_TERMINAL_PAUSE_MILLIS)
        .ok_or_else(|| {
            voice_sherpa::SpeechError::Audio(
                "Agent announcement pause length overflowed".to_string(),
            )
        })?
        / 1_000;
    let pause_bytes = pause_frames
        .checked_mul(usize::from(name_pcm.channels))
        .and_then(|length| length.checked_mul(bytes_per_sample))
        .ok_or_else(|| {
            voice_sherpa::SpeechError::Audio(
                "Agent announcement pause byte length overflowed".to_string(),
            )
        })?;
    let merged_length = name_pcm
        .bytes
        .len()
        .checked_add(pause_bytes)
        .and_then(|length| length.checked_add(outcome_pcm.bytes.len()))
        .ok_or_else(|| {
            voice_sherpa::SpeechError::Audio("Agent announcement PCM length overflowed".to_string())
        })?;
    let mut bytes = Vec::with_capacity(merged_length);
    bytes.extend_from_slice(&name_pcm.bytes);
    bytes.resize(name_pcm.bytes.len() + pause_bytes, 0);
    bytes.extend_from_slice(&outcome_pcm.bytes);
    PcmAudioChunk::new(
        name_pcm.format,
        name_pcm.sample_rate_hz,
        name_pcm.channels,
        bytes,
    )
    .map_err(|error| {
        voice_sherpa::SpeechError::Audio(format!("Agent announcement PCM merge failed: {error:?}"))
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/agent_announcement_synthesis_tests.rs"]
mod tests;
