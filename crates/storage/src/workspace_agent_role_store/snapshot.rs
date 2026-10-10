use super::*;

impl WorkspaceAgentRoleStore {
    /// Reads one consistent registration snapshot without creating or migrating a database.
    pub fn snapshot(&self, workspace_id: &str) -> StorageResult<WorkspaceRoleSnapshot> {
        let mut snapshot = WorkspaceRoleSnapshot {
            workspace_id: workspace_id.into(),
            revision: 0,
            schema_present: false,
            roles: Vec::new(),
            call_keys: Vec::new(),
        };
        let Some(mut connection) = self.read_connection()? else {
            return Ok(snapshot);
        };
        let tx = connection.transaction()?;
        snapshot.schema_present = true;
        snapshot.revision = tx
            .query_row(
                "SELECT revision FROM workspace_agent_role_revisions WHERE workspace_id=?1",
                [workspace_id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0);
        snapshot.roles = tx.prepare(&format!("SELECT {ROLE_COLUMNS} FROM workspace_agent_roles WHERE workspace_id=?1 ORDER BY area_id,node_id,card_instance_id"))?.query_map([workspace_id],decode_role)?.collect::<Result<Vec<_>,_>>()?;
        let rows = tx.prepare("SELECT normalized_key,display,kind,state,card_instance_id FROM workspace_agent_call_keys WHERE workspace_id=?1 ORDER BY normalized_key")?
            .query_map([workspace_id],|row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?)))?
            .collect::<Result<Vec<_>,_>>()?;
        for (key, display, kind, state, card_id) in rows {
            snapshot.call_keys.push(WorkspaceCallKey {
                normalized_key: key,
                display,
                card_instance_id: card_id,
                kind: match kind.as_str() {
                    "name" => WorkspaceCallKeyKind::Name,
                    "speech_alias" => WorkspaceCallKeyKind::SpeechAlias,
                    _ => return Err(invalid("invalid_call_key_kind")),
                },
                state: match state.as_str() {
                    "active" => WorkspaceCallKeyState::Active,
                    "inactive" => WorkspaceCallKeyState::Inactive,
                    _ => return Err(invalid("invalid_call_key_state")),
                },
            });
        }
        Ok(snapshot)
    }
}

fn invalid(code: &str) -> StorageError {
    StorageError::InvalidWorkspaceAgentRole(code.into())
}
