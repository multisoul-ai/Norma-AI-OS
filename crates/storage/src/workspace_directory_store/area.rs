use super::*;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

impl WorkspaceDirectoryStore {
    pub(super) fn read_area(
        &self,
        workspace: &str,
        area: &CatalogArea,
        project: &WorkspaceProjectRef,
        roles: &WorkspaceRoleSnapshot,
    ) -> StorageResult<Option<Vec<WorkspaceDirectoryRole>>> {
        let registered: Vec<_> = roles
            .roles
            .iter()
            .filter(|r| r.card.area_id == area.id)
            .collect();
        let path = desktop_canvas_path(&self.path, workspace, &area.id)?;
        let Some(connection) = readonly(&path)? else {
            return Ok(None);
        };
        if !tables_present(&connection, &["canvas_snapshots"])? {
            return Ok(None);
        }
        let json: Option<String> = connection
            .query_row("SELECT json FROM canvas_snapshots WHERE id=1", [], |row| {
                row.get(0)
            })
            .optional()?;
        let Some(json) = json else {
            return Ok(None);
        };
        // Deserialize only; Document::from_snapshot and identity loaders mutate legacy identity.
        let snapshot: canvas::DocumentSnapshot = serde_json::from_str(&json)?;
        let definitions = if snapshot
            .nodes
            .iter()
            .any(|n| matches!(&n.kind, canvas::NodeKind::Agent(s) if s.definition_id.is_some()))
        {
            self.definition_enablement(area)?
        } else {
            HashMap::new()
        };
        let mut projected = Vec::new();
        let mut seen = HashSet::new();
        for node in snapshot.nodes {
            let canvas::NodeKind::Agent(spec) = node.kind else {
                continue;
            };
            let card = WorkspaceCardRef {
                workspace_id: workspace.into(),
                area_id: area.id.clone(),
                node_id: node.id,
                card_instance_id: node.card_instance_id,
            };
            if card.validate().is_err() || !seen.insert(card.card_instance_id.clone()) {
                return Ok(None);
            }
            let Some(role) = registered.iter().find(|r| r.card == card) else {
                return Ok(None);
            };
            if spec.alias.as_deref() != Some(&role.name) {
                return Ok(None);
            }
            let configured_enabled = if let Some(id) = spec.definition_id {
                let Some(enabled) = definitions.get(&id) else {
                    return Ok(None);
                };
                *enabled
            } else {
                true
            };
            projected.push(WorkspaceDirectoryRole {
                card,
                project: project.clone(),
                name: role.name.clone(),
                speech_alias: role.speech_alias.clone(),
                enabled: role.enabled && configured_enabled,
                submission: WorkspaceSubmissionCapability::Supported,
                execution: None,
            });
        }
        if projected.len() != registered.len() {
            return Ok(None);
        }
        Ok(Some(projected))
    }

    fn definition_enablement(&self, area: &CatalogArea) -> StorageResult<HashMap<i64, bool>> {
        let Some(parent) = self.path.parent() else {
            return Ok(HashMap::new());
        };
        let path = parent.join("desktop-project-definitions").join(format!(
            "{:x}.sqlite",
            Sha256::digest(area.project_id.as_bytes())
        ));
        let Some(c) = readonly(&path)? else {
            return Ok(HashMap::new());
        };
        if !tables_present(&c, &["agent_definitions"])? {
            return Ok(HashMap::new());
        }
        let mut q = c.prepare("SELECT id,enabled FROM agent_definitions")?;
        let records = q
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<HashMap<_, _>, _>>()?;
        Ok(records)
    }
}
