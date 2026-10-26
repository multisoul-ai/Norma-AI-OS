use super::*;

/// Host keeps this behind a short-lived lock shared by state writes and execution.
/// Checking never authorizes an operation; it can only deny one.
pub struct ContextExecutionGate {
    scope: ContextScope,
    revision: u64,
    state: Option<WorkingState>,
}

impl ContextExecutionGate {
    pub fn new(scope: ContextScope) -> Self {
        Self {
            scope,
            revision: 0,
            state: None,
        }
    }
    pub fn begin_write(&mut self) {
        self.state = None;
    }
    pub fn install(&mut self, snapshot: ContextSnapshot) -> Result<(), &'static str> {
        if snapshot.scope != self.scope || snapshot.revision < self.revision {
            self.state = None;
            return Err("stale_context_snapshot");
        }
        self.revision = snapshot.revision;
        self.state = Some(snapshot.state);
        Ok(())
    }
    pub fn check(
        &self,
        operation: ContextOperation,
        target: Option<&str>,
    ) -> Result<(), &'static str> {
        self.checked_state(operation, target).map(|_| ())
    }

    pub fn checked_state(
        &self,
        operation: ContextOperation,
        target: Option<&str>,
    ) -> Result<&WorkingState, &'static str> {
        let state = self.state.as_ref().ok_or("context_state_unavailable")?;
        check_context_operation(state, operation, target)
            .map_err(|_| "context_operation_restricted")?;
        Ok(state)
    }
}
