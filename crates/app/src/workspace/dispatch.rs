//! Shared workspace submission boundary. Entry adapters freeze provenance; models only plan text.
pub(crate) mod authority;
mod delivery;
pub(crate) mod journal;
mod preparation;

use agent::{workspace_plan::build_original_target_manifest, WorkspaceTaskPlan};
use gpui::{App, Task};
use norma::{workspace_dispatch::*, workspace_operation::*};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use storage::{WorkspaceOperationReceipt, WorkspaceOperationStore};

use journal::shared_journal;

pub struct WorkspaceDispatchEntry {
    source: WorkspaceOperationSource,
    mode: WorkspaceRoutingMode,
    mentions: Vec<WorkspaceStructuredMention>,
}

impl WorkspaceDispatchEntry {
    /// Called by the desktop send adapter, never deserialized from model tool parameters.
    pub fn desktop(
        source: WorkspaceOperationSource,
        mode: WorkspaceRoutingMode,
        mentions: Vec<WorkspaceStructuredMention>,
    ) -> Result<Self, String> {
        source.validate().map_err(str::to_owned)?;
        if !matches!(source.origin, WorkspaceOperationOrigin::Desktop { .. }) {
            return Err("origin_authorization_mismatch".into());
        }
        Ok(Self {
            source,
            mode,
            mentions,
        })
    }

    fn fingerprint(&self) -> Result<String, String> {
        serde_json::to_vec(&(&self.source, self.mode, &self.mentions))
            .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
            .map_err(|error| error.to_string())
    }
}

pub enum WorkspaceDispatchPreparation {
    Existing(WorkspaceOperationReceipt),
    Ready(PendingWorkspaceDispatch),
    NeedsTarget(WorkspaceResolution),
}

/// Bound once before planning. Cannot be deserialized, cloned or retargeted by an entry/model.
pub struct PendingWorkspaceDispatch {
    entry: WorkspaceDispatchEntry,
    fingerprint: String,
    manifest: OriginalTargetManifest,
    resolution: WorkspaceResolution,
    targets: Vec<FrozenWorkspaceTarget>,
}

impl PendingWorkspaceDispatch {
    pub fn cards(&self) -> Vec<WorkspaceCardRef> {
        self.targets
            .iter()
            .map(|target| target.card.clone())
            .collect()
    }
}

#[derive(Clone)]
pub struct WorkspaceDispatchService {
    database: PathBuf,
    hook_paths: norma::agent_hook::HookInstallPaths,
}

impl WorkspaceDispatchService {
    pub fn new(database: PathBuf) -> Self {
        Self {
            database,
            hook_paths: norma::agent_hook::default_install_paths(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_hook_paths(mut self, paths: norma::agent_hook::HookInstallPaths) -> Self {
        self.hook_paths = paths;
        self
    }

    /// Idempotency precedes directory resolution and cold preparation. This method starts no PTY.
    pub fn prepare(
        &self,
        entry: WorkspaceDispatchEntry,
        cx: &mut App,
    ) -> Task<Result<WorkspaceDispatchPreparation, String>> {
        let database = self.database.clone();
        cx.spawn(async move |cx| {
            let fingerprint = entry.fingerprint()?;
            let path = database.clone();
            let source = entry.source.clone();
            let original = fingerprint.clone();
            let existing = cx
                .background_executor()
                .spawn(async move {
                    WorkspaceOperationStore::new(path)
                        .lookup_original_request(&source, &original)
                        .map_err(|error| error.to_string())
                })
                .await?;
            if let Some(receipt) = existing {
                return Ok(WorkspaceDispatchPreparation::Existing(receipt));
            }
            let mut directory = read_directory(&database, &entry.source.workspace_id, cx).await?;
            cx.update(|cx| {
                super::directory::project_retained_directory(&database, &mut directory, cx)
            })
            .map_err(|error| error.to_string())?;
            let manifest = build_original_target_manifest(
                &directory,
                entry.mode,
                &entry.source.original_text,
                &entry.mentions,
            );
            let resolution = runtime::workspace_resolution::resolve_workspace_targets(
                &directory,
                &WorkspaceResolutionRequest {
                    mode: manifest.mode,
                    body: manifest.original_body.clone(),
                    targets: manifest.targets.clone(),
                },
            );
            let WorkspaceResolution::Ready { targets, .. } = &resolution else {
                return Ok(WorkspaceDispatchPreparation::NeedsTarget(resolution));
            };
            agent::workspace_plan::validate_workspace_task_plan(&manifest, &resolution, targets)
                .map_err(|error| format!("invalid_original_plan:{error:?}"))?;
            Ok(WorkspaceDispatchPreparation::Ready(
                PendingWorkspaceDispatch {
                    targets: targets.clone(),
                    entry,
                    fingerprint,
                    manifest,
                    resolution,
                },
            ))
        })
    }

    /// All planning must complete before this single call. No second model chooses the recipients.
    /// The operation owns preparation/delivery once submitted. Dropping this receipt waiter
    /// cannot cancel a committed group or strand a reserved native launch; explicit cancellation
    /// must use the assignment control boundary and its persisted evidence.
    pub fn submit(
        &self,
        pending: PendingWorkspaceDispatch,
        plan: WorkspaceTaskPlan,
        cx: &mut App,
    ) -> Task<Result<WorkspaceOperationReceipt, String>> {
        let database = self.database.clone();
        let hook_paths = self.hook_paths.clone();
        let (sender, receiver) = futures::channel::oneshot::channel();
        cx.spawn(async move |cx| {
            let result = delivery::submit(database, pending, plan, hook_paths, cx).await;
            let _ = sender.send(result);
        })
        .detach();
        cx.spawn(async move |_| {
            receiver
                .await
                .map_err(|_| "workspace_dispatch_stopped".to_owned())?
        })
    }
}

async fn read_directory(
    database: &std::path::Path,
    workspace: &str,
    cx: &mut gpui::AsyncApp,
) -> Result<WorkspaceDirectory, String> {
    let path = database.to_owned();
    let workspace = workspace.to_owned();
    cx.background_executor()
        .spawn(async move {
            storage::WorkspaceDirectoryStore::new(path)
                .read(&workspace, "desktop", None)
                .map_err(|error| error.to_string())
        })
        .await
}
