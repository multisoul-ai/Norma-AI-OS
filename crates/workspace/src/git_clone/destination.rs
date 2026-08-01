use std::fmt;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloneDestination {
    parent: PathBuf,
    target: PathBuf,
}

impl CloneDestination {
    pub fn parent(&self) -> &Path {
        &self.parent
    }

    pub fn target(&self) -> &Path {
        &self.target
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloneDestinationError {
    InvalidFolderName,
    ParentDoesNotExist,
    ParentNotDirectory,
    ParentReadOnly,
    CouldNotResolveParent,
    TargetAlreadyExists,
}

impl fmt::Display for CloneDestinationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidFolderName => "Choose a valid folder name without path separators.",
            Self::ParentDoesNotExist => "The selected parent folder does not exist.",
            Self::ParentNotDirectory => "The selected destination is not a folder.",
            Self::ParentReadOnly => "The selected parent folder is read-only.",
            Self::CouldNotResolveParent => "The selected parent folder could not be resolved.",
            Self::TargetAlreadyExists => {
                "A file or folder with this project name already exists at the destination."
            }
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for CloneDestinationError {}

pub fn validate_clone_destination(
    parent: &Path,
    folder_name: &str,
) -> Result<CloneDestination, CloneDestinationError> {
    validate_folder_name(folder_name)?;
    if !parent.exists() {
        return Err(CloneDestinationError::ParentDoesNotExist);
    }
    if !parent.is_dir() {
        return Err(CloneDestinationError::ParentNotDirectory);
    }
    let metadata = parent
        .metadata()
        .map_err(|_| CloneDestinationError::CouldNotResolveParent)?;
    if metadata.permissions().readonly() {
        return Err(CloneDestinationError::ParentReadOnly);
    }
    let parent = parent
        .canonicalize()
        .map_err(|_| CloneDestinationError::CouldNotResolveParent)?;
    let target = parent.join(folder_name);
    if target.exists() {
        return Err(CloneDestinationError::TargetAlreadyExists);
    }
    Ok(CloneDestination { parent, target })
}

fn validate_folder_name(folder_name: &str) -> Result<(), CloneDestinationError> {
    if folder_name.is_empty()
        || folder_name.trim() != folder_name
        || folder_name.contains(['/', '\\'])
        || folder_name.chars().any(char::is_control)
    {
        return Err(CloneDestinationError::InvalidFolderName);
    }
    let mut components = Path::new(folder_name).components();
    let is_single_normal_component =
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();
    if !is_single_normal_component || matches!(folder_name, "." | "..") {
        return Err(CloneDestinationError::InvalidFolderName);
    }
    Ok(())
}
