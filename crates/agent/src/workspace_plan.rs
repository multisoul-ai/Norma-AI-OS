//! Full-plan validation against the original source; planning itself performs no input.
mod original;
mod selection;
use norma::workspace_dispatch::*;
pub use original::build_original_target_manifest;
pub use selection::*;
use std::collections::HashSet;

/// `expected` is pure routing of the source manifest, not the model's rewritten targets.
/// Call once after all planning succeeds and before a single atomic batch acceptance.
pub fn validate_workspace_task_plan(
    manifest: &OriginalTargetManifest,
    expected: &WorkspaceResolution,
    planned: &[FrozenWorkspaceTarget],
) -> Result<(), WorkspacePlanError> {
    if manifest.mode != WorkspaceRoutingMode::Execute {
        return Err(WorkspacePlanError::ModeDoesNotDispatch);
    }
    if !manifest.complete {
        return Err(WorkspacePlanError::AmbiguousOriginalTargets);
    }
    validate_original_evidence(manifest)?;
    let WorkspaceResolution::Ready { body, targets } = expected else {
        return Err(WorkspacePlanError::UnresolvedOriginalTargets);
    };
    if body != &manifest.original_body {
        return Err(WorkspacePlanError::SourceBodyMismatch);
    }
    let expected: HashSet<_> = targets
        .iter()
        .map(|t| (&t.card, &t.project, &t.execution))
        .collect();
    let actual: HashSet<_> = planned
        .iter()
        .map(|t| (&t.card, &t.project, &t.execution))
        .collect();
    if expected.is_empty() || actual.len() != planned.len() || expected != actual {
        return Err(WorkspacePlanError::TargetSetMismatch);
    }
    Ok(())
}

fn validate_original_evidence(manifest: &OriginalTargetManifest) -> Result<(), WorkspacePlanError> {
    if manifest.evidence.iter().any(|e| {
        manifest.original_body.get(e.start_byte..e.end_byte) != Some(e.literal.as_str())
            || !manifest.targets.contains(&e.target)
    }) {
        return Err(WorkspacePlanError::InvalidEvidence);
    }
    if manifest
        .targets
        .iter()
        .any(|t| !manifest.evidence.iter().any(|e| e.target == *t))
    {
        return Err(WorkspacePlanError::InvalidEvidence);
    }
    Ok(())
}
