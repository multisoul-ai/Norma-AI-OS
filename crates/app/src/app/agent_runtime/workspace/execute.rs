use super::*;
use std::path::Path;

/// Final foreground boundary after asynchronous preparation and authentication.
pub(super) fn execute_ready(
    database: &Path,
    scope: &Scope,
    command: AdapterCommand,
    lifetime: &CancellationToken,
    turn: &CancellationToken,
    cx: &mut App,
) -> Result<AdapterResult, AdapterError> {
    if lifetime.is_cancelled() || turn.is_cancelled() {
        return Err(AdapterError::Cancelled);
    }
    crate::mobile::workspace_live::execute_scoped(
        database,
        &scope.areas,
        &scope.device,
        &scope.operation,
        command,
        cx,
    )
}

#[cfg(test)]
#[path = "../../../../tests/unit/workspace_memory_dispatch_tests.rs"]
mod tests;
