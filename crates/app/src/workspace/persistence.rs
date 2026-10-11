use crate::app::MultisoulApp;
use crate::canvas::master_layout::MasterLayoutState;
use runtime::Runtime;
use std::time::Duration;
use storage::VoiceHistoryEntry;
use storage::{ProjectStore, StorageError};

const PERSISTENCE_DEBOUNCE: Duration = Duration::from_millis(360);

#[derive(Clone, Debug)]
pub(crate) struct CardPublication {
    generation: u64,
    path: Option<std::path::PathBuf>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PersistenceSaveState {
    Clean,
    Pending,
    Failed,
}

#[derive(Debug, Default)]
pub(crate) struct PersistenceScheduler {
    project_store: Option<ProjectStore>,
    agent_dispatch_writer: Option<storage::BackgroundAgentDispatchStore>,
    canvas_dirty: bool,
    canvas_publications_pending: usize,
    canvas_publication_generation: u64,
    canvas_publication_uncertain: bool,
    role_removals: Vec<(
        storage::WorkspaceAgentRoleStore,
        agent::workspace_dispatch::WorkspaceCardRef,
    )>,
    session_dirty: bool,
    dirty_for: Duration,
    last_save_failed: bool,
    pub(crate) retained_checkpoints: Vec<storage::SessionCheckpoint>,
}

impl PersistenceScheduler {
    pub(crate) fn set_project_store(&mut self, store: ProjectStore) {
        let previous_writer = self.agent_dispatch_writer.take();
        self.agent_dispatch_writer = Some(storage::BackgroundAgentDispatchStore::new_after(
            previous_writer,
            store.agent_dispatch_task_store(),
        ));
        self.project_store = Some(store);
        self.retained_checkpoints.clear();
        self.role_removals.clear();
        self.canvas_dirty = false;
        self.canvas_publications_pending = 0;
        self.canvas_publication_generation = self.canvas_publication_generation.wrapping_add(1);
        self.canvas_publication_uncertain = false;
        self.session_dirty = false;
        self.dirty_for = Duration::ZERO;
        self.last_save_failed = false;
    }

    pub(crate) fn project_store(&self) -> Option<&ProjectStore> {
        self.project_store.as_ref()
    }

    pub(crate) fn queue_role_removal(
        &mut self,
        store: storage::WorkspaceAgentRoleStore,
        card: agent::workspace_dispatch::WorkspaceCardRef,
    ) {
        self.role_removals.push((store, card));
    }

    pub(crate) fn begin_card_publication(&mut self) -> CardPublication {
        self.canvas_publications_pending += 1;
        CardPublication {
            generation: self.canvas_publication_generation,
            path: self.project_store.as_ref().map(|p| p.path().to_owned()),
        }
    }

    pub(crate) fn end_card_publication(&mut self, publication: &CardPublication) {
        self.finish_card_publication(publication, true, false);
    }

    /// An expired callback does not prove deletion. Freeze stale writers until recovery/reload.
    pub(crate) fn finish_card_publication(
        &mut self,
        publication: &CardPublication,
        installed: bool,
        definitely_absent: bool,
    ) {
        if self.project_store.as_ref().map(|p| p.path()) != publication.path.as_deref() {
            return;
        }
        if !installed && !definitely_absent {
            // Even a newer binding to the same DB may have loaded before the late append.
            self.canvas_publication_uncertain = true;
        } else if self.canvas_publication_generation == publication.generation {
            self.canvas_publications_pending = self.canvas_publications_pending.saturating_sub(1);
        }
    }

    pub(crate) fn mark_canvas_dirty(&mut self) {
        self.canvas_dirty = true;
        self.dirty_for = Duration::ZERO;
    }

    pub(crate) fn mark_session_dirty(&mut self) {
        self.session_dirty = true;
        self.dirty_for = Duration::ZERO;
    }

    pub(crate) fn tick(
        &mut self,
        delta: Duration,
        runtime: &Runtime,
        master_layout: &MasterLayoutState,
    ) -> Result<bool, StorageError> {
        if !self.has_pending_work() {
            return Ok(false);
        }

        self.dirty_for = self.dirty_for.saturating_add(delta);
        if self.dirty_for < PERSISTENCE_DEBOUNCE {
            return Ok(false);
        }

        self.flush_now(runtime, master_layout)
    }

    pub(crate) fn flush_now(
        &mut self,
        runtime: &Runtime,
        master_layout: &MasterLayoutState,
    ) -> Result<bool, StorageError> {
        let result = self.flush_pending(runtime, master_layout);
        self.track_flush_result(result)
    }

    fn flush_pending(
        &mut self,
        runtime: &Runtime,
        master_layout: &MasterLayoutState,
    ) -> Result<bool, StorageError> {
        let Some(project_store) = self.project_store.as_ref() else {
            self.canvas_dirty = false;
            self.session_dirty = false;
            self.dirty_for = Duration::ZERO;
            return Ok(false);
        };

        let had_pending_work = self.has_pending_work();
        if !had_pending_work {
            self.dirty_for = Duration::ZERO;
            return Ok(false);
        }

        let canvas_blocked =
            self.canvas_publications_pending > 0 || self.canvas_publication_uncertain;
        if canvas_blocked && !self.session_dirty {
            return Ok(false);
        }

        if self.canvas_dirty && !canvas_blocked {
            if self.role_removals.is_empty() {
                runtime.save_canvas_snapshot(&project_store.canvas_store())?;
            } else {
                let removed = self
                    .role_removals
                    .iter()
                    .map(|(_, c)| (c.node_id, c.card_instance_id.clone()))
                    .collect::<Vec<_>>();
                project_store
                    .canvas_store()
                    .save_snapshot_with_card_removals(&runtime.document.to_snapshot(), &removed)?;
            }
            project_store
                .layout_store()
                .save_state(&master_layout.to_snapshot())?;
            let removals = std::mem::take(&mut self.role_removals);
            if !removals.is_empty() {
                // The canvas row has acknowledged permanent removal before keys can be released.
                std::thread::spawn(move || {
                    for (roles, card) in removals {
                        if let Err(error) = roles.delete_after_card_removed(&card) {
                            tracing::warn!(%error,"role removal will be reconciled on next registration");
                        }
                    }
                });
            }
        }
        if self.session_dirty {
            let checkpoints = runtime::merge_session_checkpoints(
                &runtime.document,
                runtime.collect_session_checkpoints(),
                &self.retained_checkpoints,
            );
            project_store
                .session_store()
                .replace_all_checkpoints(&checkpoints)?;
            self.retained_checkpoints = checkpoints;
        }
        // Node sessions are derived from live nodes and may require process metadata.
        // Keep them on dirty flushes only so close/no-op flushes stay off the SQLite path.
        runtime.save_node_sessions(&project_store.node_session_store())?;
        if !canvas_blocked {
            self.canvas_dirty = false;
        }
        self.session_dirty = false;
        self.dirty_for = Duration::ZERO;
        Ok(had_pending_work)
    }

    fn track_flush_result(
        &mut self,
        result: Result<bool, StorageError>,
    ) -> Result<bool, StorageError> {
        self.last_save_failed = result.is_err();
        result
    }

    pub(crate) fn save_state(&self) -> PersistenceSaveState {
        if self.last_save_failed {
            PersistenceSaveState::Failed
        } else if self.has_pending_work()
            || self.canvas_publications_pending > 0
            || self.canvas_publication_uncertain
        {
            PersistenceSaveState::Pending
        } else {
            PersistenceSaveState::Clean
        }
    }

    pub(crate) fn has_pending_work(&self) -> bool {
        self.canvas_dirty || self.session_dirty
    }

    pub(crate) fn save_voice_history(
        &self,
        entry: &VoiceHistoryEntry,
    ) -> Result<Option<i64>, StorageError> {
        let Some(project_store) = self.project_store.as_ref() else {
            return Ok(None);
        };
        project_store
            .voice_history_store()
            .save_entry(entry)
            .map(Some)
    }

    pub(crate) fn agent_dispatch_writer(&self) -> Option<&storage::BackgroundAgentDispatchStore> {
        self.agent_dispatch_writer.as_ref()
    }
}

impl MultisoulApp {
    pub(crate) fn mark_canvas_dirty(&mut self) {
        self.persistence.mark_canvas_dirty();
        crate::mobile::project_events::changed();
    }

    pub(crate) fn mark_session_dirty(&mut self) {
        self.persistence.mark_session_dirty();
        crate::mobile::project_events::changed();
    }

    pub(crate) fn tick_persistence(&mut self, delta: Duration) -> bool {
        match self
            .persistence
            .tick(delta, &self.runtime, &self.master_layout)
        {
            Ok(changed) => changed,
            Err(error) => {
                tracing::warn!(%error, "workspace persistence tick failed");
                true
            }
        }
    }

    pub(crate) fn flush_persistence_now(&mut self) {
        if let Err(error) = self
            .persistence
            .flush_now(&self.runtime, &self.master_layout)
        {
            tracing::warn!(%error, "workspace persistence flush failed");
        }
        if let Some(writer) = self.persistence.agent_dispatch_writer() {
            if let Err(error) = writer.flush() {
                tracing::warn!(%error, "Agent dispatch persistence flush failed");
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/persistence_tests.rs"]
mod tests;
