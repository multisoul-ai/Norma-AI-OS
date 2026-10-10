use super::*;

impl WorkspaceAgentRoleStore {
    /// The caller must have confirmed permanent removal of this exact card first.
    /// Releases all keys only after the AppDB transaction commits. No cross-database
    /// atomicity is implied; a failed call retains ownership and may be retried.
    pub fn delete_after_card_removed(&self, card: &WorkspaceCardRef) -> StorageResult<()> {
        validate_card(card)?;
        let mut connection = self.write_connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        required_role(&tx, card)?;
        tx.execute("DELETE FROM workspace_agent_roles WHERE workspace_id=?1 AND card_instance_id=?2 AND area_id=?3 AND node_id=?4",params![card.workspace_id,card.card_instance_id,card.area_id,card.node_id])?;
        bump_revision(&tx, &card.workspace_id)?;
        tx.commit()?;
        Ok(())
    }

    /// Atomically installs a new name pair; previously retired keys cannot be revived.
    pub fn rename(&self, card: &WorkspaceCardRef, name: &str) -> StorageResult<WorkspaceAgentRole> {
        validate_card(card)?;
        let name = parse_name(name)?;
        let mut connection = self.write_connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous = required_role(&tx, card)?;
        if previous.name == name.display() {
            return Ok(previous);
        }
        reject_name_conflicts(&tx, &card.workspace_id, &name, Some(&card.card_instance_id))?;
        tx.execute("UPDATE workspace_agent_call_keys SET state='inactive' WHERE workspace_id=?1 AND card_instance_id=?2 AND state='active'",params![card.workspace_id,card.card_instance_id])?;
        for (key, display, kind) in name_keys(&name) {
            tx.execute("INSERT INTO workspace_agent_call_keys(workspace_id,normalized_key,card_instance_id,display,kind,state)
                VALUES(?1,?2,?3,?4,?5,'active') ON CONFLICT(workspace_id,normalized_key)
                DO UPDATE SET display=excluded.display,kind=excluded.kind,state='active'",
                params![card.workspace_id,key,card.card_instance_id,display,kind])?;
        }
        tx.execute("UPDATE workspace_agent_roles SET name=?3,speech_alias=?4,updated_at_unix=?5 WHERE workspace_id=?1 AND card_instance_id=?2",params![card.workspace_id,card.card_instance_id,name.display(),name.speech_alias(),current_unix_timestamp()?])?;
        bump_revision(&tx, &card.workspace_id)?;
        let result = required_role(&tx, card)?;
        tx.commit()?;
        Ok(result)
    }

    /// Disabling a retained role changes eligibility, never its key ownership.
    pub fn set_enabled(
        &self,
        card: &WorkspaceCardRef,
        enabled: bool,
    ) -> StorageResult<WorkspaceAgentRole> {
        validate_card(card)?;
        let mut connection = self.write_connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous = required_role(&tx, card)?;
        if previous.enabled == enabled {
            return Ok(previous);
        }
        tx.execute("UPDATE workspace_agent_roles SET enabled=?3,updated_at_unix=?4 WHERE workspace_id=?1 AND card_instance_id=?2",params![card.workspace_id,card.card_instance_id,enabled,current_unix_timestamp()?])?;
        bump_revision(&tx, &card.workspace_id)?;
        let result = required_role(&tx, card)?;
        tx.commit()?;
        Ok(result)
    }
}

fn required_role(
    connection: &Connection,
    card: &WorkspaceCardRef,
) -> StorageResult<WorkspaceAgentRole> {
    read_role(connection, card)?
        .ok_or_else(|| StorageError::InvalidWorkspaceAgentRole("target_missing".into()))
}
