use gpui::{Entity, Subscription};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use ui::component::input::InputState;
#[cfg(feature = "e2e-test-support")]
use workspace::git_clone::CloneStage;
use workspace::git_clone::{
    validate_clone_destination, CloneCancellation, CloneDestination, CloneEvent, CloneFailure,
    CloneProgress, CloneTransport, GitCloneRequest, GitCloneRunner, GitHubRemote,
};

static NEXT_CLONE_TASK_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Default)]
pub(crate) struct CloneProjectUiState {
    dialog: Option<CloneProjectDialogState>,
    backend: CloneProjectBackend,
}

#[derive(Clone, Default)]
pub(super) enum CloneProjectBackend {
    #[default]
    System,
    #[cfg(feature = "e2e-test-support")]
    SuccessfulFake,
}

pub(super) struct CloneProjectDialogState {
    pub(super) task_id: u64,
    pub(super) remote_input: Entity<InputState>,
    pub(super) folder_input: Entity<InputState>,
    pub(super) remote_draft: String,
    pub(super) folder_draft: String,
    pub(super) parent: PathBuf,
    pub(super) transport: CloneTransport,
    pub(super) phase: CloneProjectPhase,
    pub(super) cancellation: Option<CloneCancellation>,
    pub(super) _subscriptions: Vec<Subscription>,
}

#[derive(Clone, Debug)]
pub(super) enum CloneProjectPhase {
    Editing,
    Running(CloneProgress),
    Cancelling,
    Failed {
        message: String,
        target_preserved: bool,
    },
}

pub(super) struct PreparedClone {
    pub(super) remote: GitHubRemote,
    pub(super) transport: CloneTransport,
    pub(super) destination: CloneDestination,
}

#[cfg(feature = "e2e-test-support")]
pub(crate) struct CloneProjectInternalE2eSnapshot {
    pub(crate) open: bool,
    pub(crate) phase: &'static str,
    pub(crate) transport: &'static str,
    pub(crate) remote: String,
    pub(crate) folder: String,
    pub(crate) parent: Option<PathBuf>,
    pub(crate) target_preview: Option<PathBuf>,
    pub(crate) validation_error: Option<String>,
}

impl CloneProjectUiState {
    pub(crate) fn is_open(&self) -> bool {
        self.dialog.is_some()
    }

    pub(super) fn dialog(&self) -> Option<&CloneProjectDialogState> {
        self.dialog.as_ref()
    }

    pub(super) fn dialog_mut(&mut self) -> Option<&mut CloneProjectDialogState> {
        self.dialog.as_mut()
    }

    pub(super) fn open(
        &mut self,
        remote_input: Entity<InputState>,
        folder_input: Entity<InputState>,
        parent: PathBuf,
        subscriptions: Vec<Subscription>,
    ) {
        let task_id = NEXT_CLONE_TASK_ID.fetch_add(1, Ordering::Relaxed);
        self.dialog = Some(CloneProjectDialogState {
            task_id,
            remote_input,
            folder_input,
            remote_draft: String::new(),
            folder_draft: String::new(),
            parent,
            transport: CloneTransport::Https,
            phase: CloneProjectPhase::Editing,
            cancellation: None,
            _subscriptions: subscriptions,
        });
    }

    pub(super) fn close(&mut self) {
        self.dialog = None;
    }

    pub(super) fn prepare(&self) -> Result<PreparedClone, String> {
        let dialog = self
            .dialog
            .as_ref()
            .ok_or_else(|| "Clone dialog is not open.".to_string())?;
        let remote =
            GitHubRemote::parse(&dialog.remote_draft).map_err(|error| error.to_string())?;
        let folder = effective_folder_name(&remote, &dialog.folder_draft);
        let destination = validate_clone_destination(&dialog.parent, folder)
            .map_err(|error| error.to_string())?;
        Ok(PreparedClone {
            remote,
            transport: dialog.transport,
            destination,
        })
    }

    pub(super) fn preview_target(&self) -> Option<PathBuf> {
        let dialog = self.dialog.as_ref()?;
        let remote = GitHubRemote::parse(&dialog.remote_draft).ok()?;
        Some(
            dialog
                .parent
                .join(effective_folder_name(&remote, &dialog.folder_draft)),
        )
    }

    pub(super) fn is_busy(&self) -> bool {
        self.dialog.as_ref().is_some_and(|dialog| {
            matches!(
                dialog.phase,
                CloneProjectPhase::Running(_) | CloneProjectPhase::Cancelling
            )
        })
    }

    pub(super) fn backend(&self) -> CloneProjectBackend {
        self.backend.clone()
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn use_successful_fake_backend(&mut self) {
        self.backend = CloneProjectBackend::SuccessfulFake;
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn set_parent_for_e2e(&mut self, parent: PathBuf) {
        if let Some(dialog) = self.dialog.as_mut() {
            dialog.parent = parent;
        }
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn e2e_snapshot(&self) -> CloneProjectInternalE2eSnapshot {
        let Some(dialog) = self.dialog.as_ref() else {
            return CloneProjectInternalE2eSnapshot {
                open: false,
                phase: "closed",
                transport: "https",
                remote: String::new(),
                folder: String::new(),
                parent: None,
                target_preview: None,
                validation_error: None,
            };
        };
        let phase = match dialog.phase {
            CloneProjectPhase::Editing => "editing",
            CloneProjectPhase::Running(_) => "running",
            CloneProjectPhase::Cancelling => "cancelling",
            CloneProjectPhase::Failed { .. } => "failed",
        };
        CloneProjectInternalE2eSnapshot {
            open: true,
            phase,
            transport: match dialog.transport {
                CloneTransport::Https => "https",
                CloneTransport::Ssh => "ssh",
            },
            remote: dialog.remote_draft.clone(),
            folder: dialog.folder_draft.clone(),
            parent: Some(dialog.parent.clone()),
            target_preview: self.preview_target(),
            validation_error: if dialog.remote_draft.trim().is_empty() {
                None
            } else {
                self.prepare().err()
            },
        }
    }
}

impl CloneProjectBackend {
    pub(super) fn run(
        self,
        request: GitCloneRequest,
        cancellation: &CloneCancellation,
        on_event: impl FnMut(CloneEvent),
    ) -> Result<PathBuf, CloneFailure> {
        #[cfg(feature = "e2e-test-support")]
        let mut on_event = on_event;
        match self {
            Self::System => GitCloneRunner::system().run(request, cancellation, on_event),
            #[cfg(feature = "e2e-test-support")]
            Self::SuccessfulFake => {
                let target = request.target().to_path_buf();
                std::fs::create_dir(&target)
                    .expect("E2E fake clone backend should create the validated target");
                std::fs::write(target.join("README.md"), b"# Cloned by E2E fake\n")
                    .expect("E2E fake clone backend should materialize a tracked fixture file");
                on_event(CloneEvent::Progress(CloneProgress {
                    stage: CloneStage::Receiving,
                    percent: Some(100),
                }));
                Ok(target)
            }
        }
    }
}

pub(super) fn default_clone_parent(current_root: Option<&Path>) -> PathBuf {
    current_root
        .and_then(|path| path.parent())
        .map(Path::to_path_buf)
        .or_else(dirs::document_dir)
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn effective_folder_name<'a>(remote: &'a GitHubRemote, folder_draft: &'a str) -> &'a str {
    let folder = folder_draft.trim();
    if folder.is_empty() {
        remote.repository()
    } else {
        folder
    }
}
