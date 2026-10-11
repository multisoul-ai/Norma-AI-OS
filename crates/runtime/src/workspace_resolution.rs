//! Pure full-workspace target selection. No storage, loading, creation or input.
use norma::workspace_dispatch::*;
use std::collections::HashSet;

pub fn resolve_workspace_targets(
    directory: &WorkspaceDirectory,
    request: &WorkspaceResolutionRequest,
) -> WorkspaceResolution {
    if request.mode != WorkspaceRoutingMode::Execute {
        return WorkspaceResolution::NoDispatch {
            mode: request.mode,
            body: request.body.clone(),
        };
    }
    let failure = |reason, candidates| WorkspaceResolution::NeedsTarget {
        body: request.body.clone(),
        reason,
        candidates,
    };
    if !directory.is_complete() {
        return failure(WorkspaceResolutionReason::DirectoryIncomplete, vec![]);
    }
    if !directory.is_consistent() {
        return failure(WorkspaceResolutionReason::InvalidDirectory, vec![]);
    }
    if request.body.trim().is_empty() {
        return failure(WorkspaceResolutionReason::EmptyTask, vec![]);
    }
    if request.targets.is_empty() {
        return failure(WorkspaceResolutionReason::MissingTarget, vec![]);
    }
    let mut targets = Vec::new();
    let mut seen = HashSet::new();
    for intent in &request.targets {
        match resolve_one(directory, intent) {
            Ok(role) => {
                if seen.insert(&role.card) {
                    targets.push(freeze(directory, role));
                }
            }
            Err((reason, candidates)) => return failure(reason, candidates),
        }
    }
    WorkspaceResolution::Ready {
        body: request.body.clone(),
        targets,
    }
}

/// Recheck only the frozen address, never rerun name/project selection.
pub fn revalidate_workspace_binding(
    directory: &WorkspaceDirectory,
    target: &FrozenWorkspaceTarget,
) -> Result<(), WorkspaceResolutionReason> {
    if !directory.is_complete() {
        return Err(WorkspaceResolutionReason::DirectoryIncomplete);
    }
    // An old key may still describe a removed/recreated card; identity takes precedence.
    let role = directory
        .roles
        .iter()
        .find(|r| r.card == target.card)
        .ok_or(WorkspaceResolutionReason::TargetMissing)?;
    if !directory.is_consistent() {
        return Err(WorkspaceResolutionReason::InvalidDirectory);
    }
    if role.project != target.project {
        return Err(WorkspaceResolutionReason::ProjectMismatch);
    }
    eligible(role)?;
    if target.execution.is_some() && target.execution != role.execution {
        return Err(WorkspaceResolutionReason::ExecutionChanged);
    }
    Ok(())
}

type ResolutionFailure = (WorkspaceResolutionReason, Vec<WorkspaceDirectoryRole>);
fn resolve_one<'a>(
    directory: &'a WorkspaceDirectory,
    intent: &WorkspaceTargetIntent,
) -> Result<&'a WorkspaceDirectoryRole, ResolutionFailure> {
    match intent {
        WorkspaceTargetIntent::CallName { name, project } => {
            resolve_name(directory, name, project.as_ref())
        }
        WorkspaceTargetIntent::CallNameInProject { name, project_name } => {
            let project = project_by_name(directory, project_name).map_err(|r| (r, vec![]))?;
            resolve_name(directory, name, Some(&project.reference))
        }
        WorkspaceTargetIntent::Card { card, project } => {
            let role = directory
                .roles
                .iter()
                .find(|r| r.card == *card)
                .ok_or((WorkspaceResolutionReason::TargetMissing, vec![]))?;
            check_role(role, project.as_ref()).map_err(|r| (r, vec![]))?;
            Ok(role)
        }
        WorkspaceTargetIntent::Project { project } => resolve_project(directory, project),
        WorkspaceTargetIntent::ProjectName { name } => {
            let project = project_by_name(directory, name).map_err(|r| (r, vec![]))?;
            resolve_project(directory, &project.reference)
        }
        WorkspaceTargetIntent::Unspecified => {
            Err((WorkspaceResolutionReason::MissingTarget, vec![]))
        }
    }
}

fn resolve_name<'a>(
    directory: &'a WorkspaceDirectory,
    name: &str,
    project: Option<&WorkspaceProjectRef>,
) -> Result<&'a WorkspaceDirectoryRole, ResolutionFailure> {
    let key = normalize_workspace_call_key(name);
    let binding = directory
        .call_keys
        .iter()
        .find(|entry| entry.key == key)
        .ok_or((WorkspaceResolutionReason::UnknownName, vec![]))?;
    if binding.state == WorkspaceCallKeyStatus::Retired {
        return Err((WorkspaceResolutionReason::RetiredName, vec![]));
    }
    let role = directory
        .roles
        .iter()
        .find(|role| role.card == binding.card)
        .ok_or((WorkspaceResolutionReason::TargetMissing, vec![]))?;
    check_role(role, project).map_err(|r| (r, vec![]))?;
    Ok(role)
}

fn check_role(
    role: &WorkspaceDirectoryRole,
    project: Option<&WorkspaceProjectRef>,
) -> Result<(), WorkspaceResolutionReason> {
    if project.is_some_and(|p| role.project != *p) {
        return Err(WorkspaceResolutionReason::ProjectMismatch);
    }
    eligible(role)
}
fn eligible(role: &WorkspaceDirectoryRole) -> Result<(), WorkspaceResolutionReason> {
    if !role.enabled {
        return Err(WorkspaceResolutionReason::DisabledTarget);
    }
    if role.submission != WorkspaceSubmissionCapability::Supported {
        return Err(WorkspaceResolutionReason::UnsupportedSubmission);
    }
    Ok(())
}
fn project_by_name<'a>(
    directory: &'a WorkspaceDirectory,
    name: &str,
) -> Result<&'a WorkspaceDirectoryProject, WorkspaceResolutionReason> {
    let mut matches = directory.projects.iter().filter(|p| {
        p.name.trim().eq_ignore_ascii_case(name.trim())
            || directory.areas.iter().any(|a| {
                a.project == p.reference && a.name.trim().eq_ignore_ascii_case(name.trim())
            })
    });
    let first = matches
        .next()
        .ok_or(WorkspaceResolutionReason::ProjectMissing)?;
    if matches.next().is_some() {
        return Err(WorkspaceResolutionReason::AmbiguousProject);
    }
    Ok(first)
}
fn resolve_project<'a>(
    directory: &'a WorkspaceDirectory,
    project: &WorkspaceProjectRef,
) -> Result<&'a WorkspaceDirectoryRole, ResolutionFailure> {
    if !directory.projects.iter().any(|p| p.reference == *project) {
        return Err((WorkspaceResolutionReason::ProjectMissing, vec![]));
    }
    let candidates: Vec<_> = directory
        .roles
        .iter()
        .filter(|r| r.project == *project && eligible(r).is_ok())
        .collect();
    match candidates.as_slice() {
        [] => Err((WorkspaceResolutionReason::NoCandidates, vec![])),
        [only] => Ok(only),
        _ => Err((
            WorkspaceResolutionReason::MultipleCandidates,
            candidates.into_iter().cloned().collect(),
        )),
    }
}
fn freeze(directory: &WorkspaceDirectory, role: &WorkspaceDirectoryRole) -> FrozenWorkspaceTarget {
    FrozenWorkspaceTarget {
        card: role.card.clone(),
        project: role.project.clone(),
        name_at_binding: role.name.clone(),
        directory_revision: directory.revision,
        execution: role.execution.clone(),
    }
}
