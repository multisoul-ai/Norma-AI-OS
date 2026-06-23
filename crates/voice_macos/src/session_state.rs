use crate::SpeechError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionPhase {
    Idle,
    Recording,
    Finishing,
    Finished,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionEvent {
    Start,
    Finish,
    Cancel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionState {
    phase: SessionPhase,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            phase: SessionPhase::Idle,
        }
    }
}

impl SessionState {
    pub fn phase(&self) -> SessionPhase {
        self.phase
    }

    pub fn apply(&mut self, event: SessionEvent) -> Result<(), SpeechError> {
        match (self.phase, event) {
            (SessionPhase::Idle, SessionEvent::Start) => {
                self.phase = SessionPhase::Recording;
                Ok(())
            }
            (SessionPhase::Recording, SessionEvent::Finish) => {
                self.phase = SessionPhase::Finished;
                Ok(())
            }
            (SessionPhase::Recording, SessionEvent::Cancel) => {
                self.phase = SessionPhase::Cancelled;
                Ok(())
            }
            (SessionPhase::Recording, SessionEvent::Start) => Err(SpeechError::Provider(
                "speech session is already recording".to_string(),
            )),
            (SessionPhase::Finished, SessionEvent::Finish) => Err(SpeechError::AlreadyFinished),
            (SessionPhase::Cancelled, SessionEvent::Finish)
            | (SessionPhase::Idle, SessionEvent::Finish)
            | (SessionPhase::Idle, SessionEvent::Cancel) => Err(SpeechError::NotRecording),
            (SessionPhase::Finished, _) => Err(SpeechError::AlreadyFinished),
            (SessionPhase::Cancelled, _) => Err(SpeechError::NotRecording),
            (SessionPhase::Finishing, _) => Err(SpeechError::Provider(
                "speech session is already finishing".to_string(),
            )),
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/session_state_tests.rs"]
mod tests;
