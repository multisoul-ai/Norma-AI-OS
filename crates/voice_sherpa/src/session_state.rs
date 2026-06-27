use crate::SpeechError;

#[cfg(test)]
#[path = "../tests/unit/session_state_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionPhase {
    Idle,
    Recording,
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

impl Default for SessionPhase {
    fn default() -> Self {
        Self::Idle
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            phase: SessionPhase::default(),
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
            (SessionPhase::Finished, SessionEvent::Finish) => Err(SpeechError::AlreadyFinished),
            _ => Err(SpeechError::NotRecording),
        }
    }
}
