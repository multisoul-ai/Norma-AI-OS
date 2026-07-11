use crate::DialogueError;

#[cfg(test)]
#[path = "../tests/unit/session_tests.rs"]
mod session_tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialoguePhase {
    Idle,
    Listening,
    Thinking,
    Speaking,
}

pub struct DialogueSession {
    phase: DialoguePhase,
    models_resident: bool,
    pending_transcript: Option<String>,
}

impl DialogueSession {
    pub fn new() -> Self {
        Self {
            phase: DialoguePhase::Idle,
            models_resident: false,
            pending_transcript: None,
        }
    }

    pub fn pending_transcript(&self) -> Option<&str> {
        self.pending_transcript.as_deref()
    }

    pub(crate) fn set_pending_transcript(&mut self, text: &str) {
        self.pending_transcript = Some(text.to_string());
    }

    pub fn phase(&self) -> DialoguePhase {
        self.phase
    }

    pub fn on_listening_started(&mut self) {
        self.phase = DialoguePhase::Listening;
        self.models_resident = true;
    }

    pub fn on_listening_stopped(&mut self) {
        self.phase = DialoguePhase::Idle;
    }

    pub fn on_window_closed(&mut self) {
        self.phase = DialoguePhase::Idle;
        self.models_resident = false;
    }

    pub fn on_utterance_final(&mut self) -> Result<(), DialogueError> {
        if self.phase != DialoguePhase::Listening {
            return Err(DialogueError::Message(format!(
                "on_utterance_final requires Listening, got {:?}",
                self.phase
            )));
        }
        self.phase = DialoguePhase::Thinking;
        Ok(())
    }

    pub fn on_report_pending(&mut self) -> Result<(), DialogueError> {
        if self.phase != DialoguePhase::Listening {
            return Err(DialogueError::Message(format!(
                "on_report_pending requires Listening, got {:?}",
                self.phase
            )));
        }
        self.phase = DialoguePhase::Thinking;
        Ok(())
    }

    pub fn on_reply_ready(&mut self) -> Result<(), DialogueError> {
        if self.phase != DialoguePhase::Thinking {
            return Err(DialogueError::Message(format!(
                "on_reply_ready requires Thinking, got {:?}",
                self.phase
            )));
        }
        self.phase = DialoguePhase::Speaking;
        Ok(())
    }

    pub fn on_playback_finished_or_interrupted(&mut self, keep_listening: bool) {
        if keep_listening {
            self.phase = DialoguePhase::Listening;
        } else {
            self.phase = DialoguePhase::Idle;
        }
    }

    pub fn models_resident(&self) -> bool {
        self.models_resident
    }
}

impl Default for DialogueSession {
    fn default() -> Self {
        Self::new()
    }
}
