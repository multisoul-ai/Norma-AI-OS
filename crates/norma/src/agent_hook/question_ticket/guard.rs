//! In-memory authorization gate around the transport acceptance point, never around blocking IO.
use super::*;

pub type QuestionSendGuard =
    Arc<dyn Fn(&mut dyn FnMut()) -> Result<(), &'static str> + Send + Sync>;

#[derive(Default)]
pub(super) struct SendGuard(Option<QuestionSendGuard>);
impl std::fmt::Debug for SendGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SendGuard")
            .field("installed", &self.0.is_some())
            .finish()
    }
}

impl HookQuestion {
    /// Installed by the owning host before queuing the answer. Cursor has a separate PTY gate.
    pub fn set_send_guard(&self, guard: QuestionSendGuard) {
        self.0.guard.lock().expect("question guard poisoned").0 = Some(guard);
    }

    pub fn send_rejection(&self) -> Option<&'static str> {
        self.0
            .state
            .lock()
            .expect("question state poisoned")
            .send_rejection
    }

    pub(super) fn guarded_send<T>(
        &self,
        check: bool,
        action: impl FnOnce() -> T,
    ) -> Result<T, &'static str> {
        // Release the guard-container lock before context -> ticket lock acquisition.
        let guard = if check {
            self.0
                .guard
                .lock()
                .expect("question guard poisoned")
                .0
                .clone()
        } else {
            None
        };
        let Some(guard) = guard else {
            return Ok(action());
        };
        let mut action = Some(action);
        let mut result = None;
        let accepted = guard(&mut || {
            if let Some(action) = action.take() {
                result = Some(action());
            }
        });
        // Only gate refusal marks rejection. IO failures belong to the action's own result.
        if let Err(reason) = accepted {
            let mut state = self.0.state.lock().expect("question state poisoned");
            if state.native_send == NativeSendStatus::Unsent
                || matches!(self.0.adapter, NativeQuestion::Claude(_))
            {
                state.send_rejection = Some(reason);
                if state.native_send == NativeSendStatus::Writing {
                    state.native_send = NativeSendStatus::Unknown;
                }
                state.status = QuestionHookStatus::Cancelled;
                state.output = None;
            }
            return Err(reason);
        }
        result.ok_or("question_guard_did_not_execute")
    }
}
