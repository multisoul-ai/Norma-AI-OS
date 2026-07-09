use crate::session::{DialoguePhase, DialogueSession};

#[cfg(test)]
#[path = "../tests/unit/speaking_tests.rs"]
mod speaking_tests;

/// Plan 01 stub reply: echo the user transcript in a short Chinese acknowledgement.
pub fn format_stub_reply(transcript: &str) -> String {
    format!("收到：{transcript}")
}

/// Barge-in: user speech during assistant playback returns to Listening.
pub fn handle_barge_in_speech_start(session: &mut DialogueSession) -> bool {
    if session.phase() != DialoguePhase::Speaking {
        return false;
    }
    session.on_playback_finished_or_interrupted(true);
    true
}
