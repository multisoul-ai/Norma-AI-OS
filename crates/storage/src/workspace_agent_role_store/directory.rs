use super::*;

impl WorkspaceAgentRoleStore {
    /// Background-only scan. Includes all persisted agents, regardless of definition or state.
    /// Missing canvas files are empty areas, while malformed/unreadable files fail the scan.
    pub fn register_workspace(
        &self,
        workspace_id: &str,
    ) -> StorageResult<WorkspaceRoleReconciliation> {
        if crate::DesktopWorkspaceStore::new(self.path.clone())
            .get(workspace_id)?
            .is_none()
        {
            return Err(StorageError::InvalidWorkspaceAgentRole(
                "workspace_missing".into(),
            ));
        }
        self.recover_creations(workspace_id)?;
        let previously_registered = self.list(workspace_id)?;
        let areas =
            crate::DesktopWorkspaceStore::new(self.path.clone()).list_projects(workspace_id)?;
        let mut candidates = Vec::new();
        for area in &areas {
            let definitions = crate::ProjectStore::initialize_for_desktop_definitions(
                &crate::DatabasePaths::isolated_test_database(&self.path),
                &area.project_key,
            )?
            .agent_definition_store()
            .list()?;
            let path =
                crate::project_store::desktop_canvas_path(&self.path, workspace_id, &area.id)?;
            if !path.try_exists()? {
                continue;
            }
            if let Some(snapshot) =
                crate::CanvasStore::new(path).load_snapshot_with_card_identity()?
            {
                for node in snapshot.nodes {
                    if let canvas::CanvasNodeKind::Agent(spec) = node.kind {
                        candidates.push(WorkspaceRoleCandidate {
                            card: WorkspaceCardRef {
                                workspace_id: workspace_id.into(),
                                area_id: area.id.clone(),
                                node_id: node.id,
                                card_instance_id: node.card_instance_id,
                            },
                            preferred_name: spec.alias,
                            enabled: spec
                                .definition_id
                                .and_then(|id| definitions.iter().find(|d| d.id == id))
                                .is_none_or(|d| d.enabled),
                        });
                    }
                }
            }
        }
        if crate::DesktopWorkspaceStore::new(self.path.clone()).list_projects(workspace_id)?
            != areas
        {
            return Err(StorageError::InvalidWorkspaceAgentRole(
                "catalog_changed".into(),
            ));
        }
        // Only a complete persistent scan can establish deletion, never Runtime disappearance.
        // Rows created after this scan started are excluded; active creator intents are retained.
        for old in previously_registered {
            if areas.iter().any(|a| a.id == old.card.area_id)
                && !candidates
                    .iter()
                    .any(|c| c.card.card_instance_id == old.card.card_instance_id)
                && !self.creation_pending(&old.card)?
            {
                let path = crate::project_store::desktop_canvas_path(
                    &self.path,
                    workspace_id,
                    &old.card.area_id,
                )?;
                if crate::CanvasStore::new(path)
                    .card_removal_recorded(old.card.node_id, &old.card.card_instance_id)?
                {
                    self.delete_after_card_removed(&old.card)?;
                } else {
                    return Err(StorageError::InvalidWorkspaceAgentRole(
                        "card_storage_missing_without_removal_proof".into(),
                    ));
                }
            }
        }
        let result = self.reconcile(workspace_id, &candidates)?;
        // Apply only aliases to the latest persisted snapshots. Never save the scanned snapshot.
        for area in areas {
            let path =
                crate::project_store::desktop_canvas_path(&self.path, workspace_id, &area.id)?;
            if !path.try_exists()? {
                continue;
            }
            let aliases = result
                .roles
                .iter()
                .filter(|r| r.card.area_id == area.id)
                .map(|r| {
                    (
                        r.card.node_id,
                        r.card.card_instance_id.clone(),
                        r.name.clone(),
                    )
                })
                .collect::<Vec<_>>();
            crate::CanvasStore::new(path).patch_agent_aliases(&aliases)?;
        }
        Ok(result)
    }

    /// Call only after all canvas files in this area have been removed successfully.
    pub fn delete_after_area_removed(
        &self,
        workspace_id: &str,
        area_id: &str,
    ) -> StorageResult<()> {
        let mut c = self.write_connection()?;
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "DELETE FROM workspace_agent_roles WHERE workspace_id=?1 AND area_id=?2",
            params![workspace_id, area_id],
        )?;
        if changed > 0 {
            bump_revision(&tx, workspace_id)?;
        }
        tx.commit()?;
        Ok(())
    }
}
