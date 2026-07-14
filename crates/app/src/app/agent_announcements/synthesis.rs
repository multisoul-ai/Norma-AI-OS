use super::{AgentAnnouncement, AgentAnnouncementOutcome};
use crate::app::dialogue::synthesize_reply;
use audio::{PcmAudioChunk, PcmFormat};
use dialogue::TtsClient;

const AGENT_TERMINAL_PAUSE_MILLIS: usize = 250;
const PCM16_BYTES_PER_SAMPLE: usize = 2;

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
        }
    }
}

pub(super) fn synthesize_agent_terminal_announcement(
    tts: &mut dyn TtsClient,
    agent_name: &str,
    outcome_word: &str,
) -> Result<PcmAudioChunk, voice_sherpa::SpeechError> {
    let name_pcm = synthesize_reply(tts, agent_name)?;
    let outcome_pcm = synthesize_reply(tts, outcome_word)?;
    join_with_agent_terminal_pause(name_pcm, outcome_pcm)
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
