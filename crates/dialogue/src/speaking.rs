use crate::session::{DialoguePhase, DialogueSession};

#[cfg(test)]
#[path = "../tests/unit/speaking_tests.rs"]
mod speaking_tests;

/// Plan 01 stub reply: echo the user transcript in a short Chinese acknowledgement.
pub fn format_stub_reply(transcript: &str) -> String {
    format!("收到：{transcript}")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpeakingSpeechSource {
    UnclassifiedMicrophone,
    ConfirmedUser,
}

pub fn handle_speaking_speech_start(
    session: &mut DialogueSession,
    source: SpeakingSpeechSource,
) -> bool {
    if session.phase() != DialoguePhase::Speaking
        || source == SpeakingSpeechSource::UnclassifiedMicrophone
    {
        return false;
    }
    session.on_playback_finished_or_interrupted(true);
    true
}

/// Barge-in: user speech during assistant playback returns to Listening.
pub fn handle_barge_in_speech_start(session: &mut DialogueSession) -> bool {
    handle_speaking_speech_start(session, SpeakingSpeechSource::ConfirmedUser)
}
