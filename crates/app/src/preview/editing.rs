use canvas::NodeId;
use gpui::{Action, App, Entity, Focusable, Subscription, Window};
use std::{io::Write, path::Path};
use ui::component::input::InputState;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PreviewEditCommandKind {
    Edit,
    Save,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = multisoul, no_json)]
pub(crate) struct PreviewEditCommand {
    pub(crate) node_id: NodeId,
    pub(crate) command: PreviewEditCommandKind,
}

pub(crate) struct PreviewEditor {
    pub(crate) input: Entity<InputState>,
    pub(crate) original: String,
    pub(crate) dirty: bool,
    pub(crate) saving: bool,
    pub(crate) saved: bool,
    pub(crate) error: Option<ui::i18n::LocalizedText>,
    pub(super) _subscription: Subscription,
}

impl super::PreviewStore {
    pub(crate) fn editor_for(&self, node_id: NodeId) -> Option<&PreviewEditor> {
        self.editors.get(&node_id)
    }

    pub(crate) fn has_pending_edits(&self, node_id: NodeId) -> bool {
        self.editor_for(node_id)
            .is_some_and(|editor| editor.dirty || editor.saving)
    }

    pub(crate) fn focused_editor(&self, window: &Window, cx: &App) -> Option<NodeId> {
        self.editors.iter().find_map(|(&id, editor)| {
            editor
                .input
                .focus_handle(cx)
                .is_focused(window)
                .then_some(id)
        })
    }
}

/// Replace a text file atomically, checking the version the user actually read.
pub(super) fn save_text_file(path: &Path, original: &str, edited: &str) -> Result<(), String> {
    // ponytail: serialize rare file saves across cards; use per-path locks if throughput matters.
    static SAVE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let save = || -> anyhow::Result<()> {
        let _guard = SAVE_LOCK
            .lock()
            .map_err(|_| anyhow::anyhow!("File save lock is unavailable"))?;
        // Resolve links so saving through one keeps the link itself intact.
        let target = std::fs::canonicalize(path)?;
        let metadata = std::fs::metadata(&target)?;
        anyhow::ensure!(metadata.is_file(), "The path is not a file");
        anyhow::ensure!(!metadata.permissions().readonly(), "This file is read-only");
        let parent = target
            .parent()
            .ok_or_else(|| anyhow::anyhow!("The file's folder could not be found"))?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(edited.as_bytes())?;
        temporary
            .as_file()
            .set_permissions(metadata.permissions())?;
        temporary.as_file().sync_all()?;
        anyhow::ensure!(
            std::fs::read_to_string(&target)? == original,
            "This file was changed outside Norma. Copy your edits, cancel, then refresh the file and try again."
        );
        temporary.persist(&target)?;
        Ok(())
    };
    save().map_err(|error| format!("Could not save: {error}"))
}

#[cfg(test)]
#[path = "../../tests/unit/preview_editing_tests.rs"]
mod tests;
