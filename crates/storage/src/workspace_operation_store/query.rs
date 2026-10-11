use super::*;

impl WorkspaceOperationStore {
    /// Call after current authorization and before resolving names or preparing Runtime.
    /// The digest comes from the original entry envelope, never model planning parameters.
    pub fn lookup_original_request(
        &self,
        source: &WorkspaceOperationSource,
        original_fingerprint: &str,
    ) -> StorageResult<Option<WorkspaceOperationReceipt>> {
        validate_original_fingerprint(original_fingerprint)?;
        let receipt = self.receipt(source)?;
        if receipt.is_none() {
            self.reject_changed_lookup_scope(source)?;
        }
        if receipt.as_ref().is_some_and(|r| {
            r.source != *source || r.original_request_fingerprint != original_fingerprint
        }) {
            return Err(invalid("idempotency_conflict"));
        }
        Ok(receipt)
    }
    /// Strict readonly lookup. Does not prepare projects, migrate missing schema or update state.
    pub fn receipt(
        &self,
        source: &WorkspaceOperationSource,
    ) -> StorageResult<Option<WorkspaceOperationReceipt>> {
        source.validate().map_err(invalid)?;
        let Some(mut connection) = self.read_connection()? else {
            return Ok(None);
        };
        let tx = connection.transaction()?;
        read_operation(&tx, source)
    }
    /// Lookup before preparing any Runtime. A conflict cannot be hidden by returning old success.
    pub fn lookup(
        &self,
        source: &WorkspaceOperationSource,
        fingerprint: &str,
    ) -> StorageResult<Option<WorkspaceOperationReceipt>> {
        let receipt = self.receipt(source)?;
        if receipt.is_none() {
            self.reject_changed_lookup_scope(source)?;
        }
        if receipt
            .as_ref()
            .is_some_and(|r| r.request_fingerprint != fingerprint)
        {
            return Err(invalid("idempotency_conflict"));
        }
        Ok(receipt)
    }
    fn reject_changed_lookup_scope(&self, source: &WorkspaceOperationSource) -> StorageResult<()> {
        let Some(connection) = self.read_connection()? else {
            return Ok(());
        };
        let exists: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM workspace_operations WHERE origin_key=?1 AND operation_id=?2)",params![source.origin.key(),source.operation_id],|r|r.get(0))?;
        if exists {
            return Err(invalid("idempotency_scope_conflict"));
        }
        Ok(())
    }
    pub fn assignment(&self, id: &str) -> StorageResult<Option<StoredWorkspaceAssignment>> {
        let Some(connection) = self.read_connection()? else {
            return Ok(None);
        };
        read_assignment(&connection, id)
    }
    pub fn assignments(
        &self,
        workspace: &str,
        after_id: &str,
        limit: usize,
    ) -> StorageResult<Vec<StoredWorkspaceAssignment>> {
        let Some(connection) = self.read_connection()? else {
            return Ok(Vec::new());
        };
        let mut query = connection.prepare("SELECT assignment_id FROM workspace_operation_assignments WHERE workspace_id=?1 AND assignment_id>?2 ORDER BY assignment_id LIMIT ?3")?;
        let ids = query
            .query_map(
                params![workspace, after_id, limit.clamp(1, 256) as u64],
                |r| r.get::<_, String>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| {
                read_assignment(&connection, &id)?.ok_or_else(|| invalid("assignment_missing"))
            })
            .collect()
    }
    pub fn events(
        &self,
        workspace: &str,
        after: u64,
        limit: usize,
    ) -> StorageResult<Vec<WorkspaceOperationEvent>> {
        let Some(connection) = self.read_connection()? else {
            return Ok(Vec::new());
        };
        let mut query = connection.prepare("SELECT sequence,origin_key,operation_id,assignment_id,evidence,receipt_json,observed_at_ms FROM workspace_operation_events WHERE workspace_id=?1 AND sequence>?2 ORDER BY sequence LIMIT ?3")?;
        let rows = query
            .query_map(params![workspace, after, limit.clamp(1, 256) as u64], |r| {
                Ok((
                    r.get::<_, u64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, u64>(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(
                |(
                    cursor,
                    origin_key,
                    operation_id,
                    assignment_id,
                    evidence,
                    json,
                    observed_at_ms,
                )| {
                    Ok(WorkspaceOperationEvent {
                        cursor,
                        workspace_id: workspace.into(),
                        origin_key,
                        operation_id,
                        assignment_id,
                        evidence,
                        receipt: serde_json::from_str(&json)?,
                        observed_at_ms,
                    })
                },
            )
            .collect()
    }
}

pub(super) fn read_operation(
    connection: &Connection,
    source: &WorkspaceOperationSource,
) -> StorageResult<Option<WorkspaceOperationReceipt>> {
    let row: Option<(String,String,String,u64)> = connection.query_row("SELECT source_json,original_request_fingerprint,request_fingerprint,accepted_at_ms FROM workspace_operations WHERE origin_key=?1 AND operation_id=?2 AND workspace_id=?3",params![source.origin.key(),source.operation_id,source.workspace_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let Some((source_json, original_request_fingerprint, request_fingerprint, accepted_at_ms)) =
        row
    else {
        return Ok(None);
    };
    let mut query = connection.prepare("SELECT assignment_id FROM workspace_operation_assignments WHERE origin_key=?1 AND operation_id=?2 ORDER BY ordinal")?;
    let ids = query
        .query_map(params![source.origin.key(), source.operation_id], |r| {
            r.get::<_, String>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let assignments = ids
        .into_iter()
        .map(|id| read_assignment(connection, &id)?.ok_or_else(|| invalid("assignment_missing")))
        .collect::<StorageResult<Vec<_>>>()?;
    let cursor = connection.query_row("SELECT COALESCE(MAX(sequence),0) FROM workspace_operation_events WHERE origin_key=?1 AND operation_id=?2",params![source.origin.key(),source.operation_id],|r|r.get(0))?;
    Ok(Some(WorkspaceOperationReceipt {
        source: serde_json::from_str(&source_json)?,
        original_request_fingerprint,
        request_fingerprint,
        assignments,
        accepted_at_ms,
        cursor,
    }))
}

pub(super) fn read_assignment(
    connection: &Connection,
    id: &str,
) -> StorageResult<Option<StoredWorkspaceAssignment>> {
    let row: Option<(String,String,String,u64,u64)> = connection.query_row("SELECT prepared_json,receipt_json,input_phase,accepted_at_ms,observed_at_ms FROM workspace_operation_assignments WHERE assignment_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
    row.map(
        |(prepared, receipt, phase, accepted_at_ms, observed_at_ms)| {
            Ok(StoredWorkspaceAssignment {
                prepared: serde_json::from_str(&prepared)?,
                receipt: serde_json::from_str(&receipt)?,
                input_phase: decode_phase(&phase)?,
                accepted_at_ms,
                observed_at_ms,
            })
        },
    )
    .transpose()
}
