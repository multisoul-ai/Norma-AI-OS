use crate::clients::{AsrClient, AsrEvent, VadClient, VadEvent};
use crate::session::DialogueSession;
use crate::DialogueError;
use audio::PcmAudioChunk;

#[cfg(test)]
#[path = "../tests/unit/listening_tests.rs"]
mod listening_tests;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListeningChunkOutcome {
    pub vad_events: Vec<VadEvent>,
    pub partial_transcripts: Vec<String>,
    pub final_transcript: Option<String>,
}

pub fn process_listening_pcm_chunk<V: VadClient, A: AsrClient>(
    vad: &mut V,
    asr: &mut A,
    chunk: &PcmAudioChunk,
    speech_active: &mut bool,
) -> Result<ListeningChunkOutcome, DialogueError> {
    let vad_events = vad.push_pcm(chunk)?;
    let mut partial_transcripts = Vec::new();
    let mut final_transcript = None;
    let mut had_speech_end = false;

    for event in &vad_events {
        match event {
            VadEvent::SpeechStart => {
                *speech_active = true;
                asr.reset();
            }
            VadEvent::SpeechEnd => {
                had_speech_end = true;
                *speech_active = false;
            }
        }
    }

    if *speech_active || had_speech_end {
        for asr_event in asr.push_pcm(chunk)? {
            if let AsrEvent::Partial(text) = asr_event {
                partial_transcripts.push(text);
            }
        }
    }

    if had_speech_end {
        final_transcript = asr.end_utterance()?;
    }

    Ok(ListeningChunkOutcome {
        vad_events,
        partial_transcripts,
        final_transcript,
    })
}

pub fn apply_asr_final(session: &mut DialogueSession, text: &str) -> Result<(), DialogueError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(DialogueError::Message("empty transcript".to_string()));
    }
    session.set_pending_transcript(trimmed);
    session.on_utterance_final()
}
