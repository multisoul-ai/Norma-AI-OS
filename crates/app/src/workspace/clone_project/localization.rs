use ui::i18n::{text, LocalizedText};
use workspace::git_clone::{CloneDestinationError, CloneStage, GitHubRemoteParseError};

pub(super) fn remote_error(error: GitHubRemoteParseError) -> LocalizedText {
    LocalizedText::new(match error {
        GitHubRemoteParseError::Empty => "clone.remote_empty",
        GitHubRemoteParseError::UnsupportedScheme => "clone.remote_scheme",
        GitHubRemoteParseError::UnsupportedHost => "clone.remote_host",
        GitHubRemoteParseError::EmbeddedCredentials => "clone.remote_credentials",
        GitHubRemoteParseError::InvalidRepositoryPath => "clone.remote_path",
        GitHubRemoteParseError::InvalidOwner => "clone.remote_owner",
        GitHubRemoteParseError::InvalidRepository => "clone.remote_repository",
        GitHubRemoteParseError::InvalidFormat => "clone.remote_format",
    })
}

pub(super) fn destination_error(error: CloneDestinationError) -> LocalizedText {
    LocalizedText::new(match error {
        CloneDestinationError::InvalidFolderName => "clone.folder_name",
        CloneDestinationError::ParentDoesNotExist => "clone.parent_missing",
        CloneDestinationError::ParentNotDirectory => "clone.parent_directory",
        CloneDestinationError::ParentReadOnly => "clone.parent_readonly",
        CloneDestinationError::CouldNotResolveParent => "clone.parent_resolve",
        CloneDestinationError::TargetAlreadyExists => "clone.target_exists",
    })
}

pub(super) fn stage_label(stage: CloneStage) -> &'static str {
    text(match stage {
        CloneStage::Connecting => "clone.connecting",
        CloneStage::Enumerating => "clone.enumerating",
        CloneStage::Counting => "clone.counting",
        CloneStage::Compressing => "clone.compressing",
        CloneStage::Receiving => "clone.receiving",
        CloneStage::Resolving => "clone.resolving",
        CloneStage::CheckingOut => "clone.checkout",
    })
}
