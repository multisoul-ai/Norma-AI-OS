use crate::SpeechError;

#[cfg(test)]
#[path = "../tests/unit/session_state_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SessionPhase {
    #[default]
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionState {
    phase: SessionPhase,
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
            (SessionPhase::Recording, SessionEvent::Start) => Err(SpeechError::AlreadyFinished),
            _ => Err(SpeechError::NotRecording),
        }
    }
}
