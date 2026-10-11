use super::validate_original_evidence;
use norma::workspace_dispatch::*;
use std::collections::HashSet;

/// Only the trusted UI/controller calls the constructor after an explicit selection.
/// No serde implementation: a model/tool payload cannot mint this capability.
#[derive(Debug)]
pub struct WorkspaceTargetSelection {
    operation_scope: String,
    original: OriginalTargetManifest,
    target_index: usize,
    selected: WorkspaceTargetIntent,
}

/// Keeps immutable source evidence alongside the code-authorized routing view.
#[derive(Debug)]
pub struct CompletedWorkspaceTargetManifest {
    original: OriginalTargetManifest,
    effective: OriginalTargetManifest,
}

impl CompletedWorkspaceTargetManifest {
    pub fn original(&self) -> &OriginalTargetManifest {
        &self.original
    }

    pub fn effective(&self) -> &OriginalTargetManifest {
        &self.effective
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceCompletionError {
    UnprovedSource,
    ScopeMismatch,
    UnknownTargetIndex,
    TargetDoesNotNeedCompletion,
    InvalidSelectedTarget,
    ProjectMismatch,
    DuplicateSelection,
}

/// `operation_scope` is code-built from the frozen source workspace, origin,
/// operation and message IDs. It must never be supplied by a model plan.
pub fn confirm_workspace_target_selection(
    operation_scope: &str,
    original: &OriginalTargetManifest,
    target_index: usize,
    directory: &WorkspaceDirectory,
    selected_card: &WorkspaceCardRef,
) -> Result<WorkspaceTargetSelection, WorkspaceCompletionError> {
    if operation_scope.is_empty() || operation_scope.chars().any(char::is_control) {
        return Err(WorkspaceCompletionError::ScopeMismatch);
    }
    if original.mode != WorkspaceRoutingMode::Execute
        || !original.complete
        || validate_original_evidence(original).is_err()
    {
        return Err(WorkspaceCompletionError::UnprovedSource);
    }
    let target = original
        .targets
        .get(target_index)
        .ok_or(WorkspaceCompletionError::UnknownTargetIndex)?;
    let name = match target {
        WorkspaceTargetIntent::CallName { name, .. }
        | WorkspaceTargetIntent::CallNameInProject { name, .. } => Some(name),
        WorkspaceTargetIntent::Project { .. } | WorkspaceTargetIntent::ProjectName { .. } => None,
        _ => return Err(WorkspaceCompletionError::TargetDoesNotNeedCompletion),
    };
    if name.is_some_and(|name| {
        directory.call_keys.iter().any(|k| {
            k.key == normalize_workspace_call_key(name) && k.state == WorkspaceCallKeyStatus::Active
        })
    }) {
        return Err(WorkspaceCompletionError::TargetDoesNotNeedCompletion);
    }
    if !directory.is_complete() || !directory.is_consistent() {
        return Err(WorkspaceCompletionError::InvalidSelectedTarget);
    }
    let selected = directory
        .roles
        .iter()
        .find(|r| &r.card == selected_card)
        .filter(|r| r.enabled && r.submission == WorkspaceSubmissionCapability::Supported)
        .ok_or(WorkspaceCompletionError::InvalidSelectedTarget)?;
    let project_matches = match target {
        WorkspaceTargetIntent::CallName { project, .. } => {
            project.as_ref().is_none_or(|p| p == &selected.project)
        }
        WorkspaceTargetIntent::Project { project } => project == &selected.project,
        WorkspaceTargetIntent::CallNameInProject { project_name, .. }
        | WorkspaceTargetIntent::ProjectName { name: project_name } => {
            directory.projects.iter().any(|p| {
                p.reference == selected.project
                    && (p.name.trim().eq_ignore_ascii_case(project_name.trim())
                        || directory.areas.iter().any(|a| {
                            a.project == p.reference
                                && a.name.trim().eq_ignore_ascii_case(project_name.trim())
                        }))
            })
        }
        _ => false,
    };
    if !project_matches {
        return Err(WorkspaceCompletionError::ProjectMismatch);
    }
    Ok(WorkspaceTargetSelection {
        operation_scope: operation_scope.into(),
        original: original.clone(),
        target_index,
        selected: WorkspaceTargetIntent::Card {
            card: selected.card.clone(),
            project: Some(selected.project.clone()),
        },
    })
}

pub fn apply_workspace_target_selections(
    operation_scope: &str,
    original: &OriginalTargetManifest,
    selections: &[WorkspaceTargetSelection],
) -> Result<CompletedWorkspaceTargetManifest, WorkspaceCompletionError> {
    let mut effective = original.clone();
    let mut changed = HashSet::new();
    for selection in selections {
        if selection.operation_scope != operation_scope || &selection.original != original {
            return Err(WorkspaceCompletionError::ScopeMismatch);
        }
        if !changed.insert(selection.target_index) {
            return Err(WorkspaceCompletionError::DuplicateSelection);
        }
        let previous = &original.targets[selection.target_index];
        effective.targets[selection.target_index] = selection.selected.clone();
        for evidence in &mut effective.evidence {
            if &evidence.target == previous {
                evidence.target = selection.selected.clone();
            }
        }
    }
    Ok(CompletedWorkspaceTargetManifest {
        original: original.clone(),
        effective,
    })
}
