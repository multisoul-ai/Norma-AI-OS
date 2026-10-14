use super::*;
use serde::Deserialize;

/// Immutable thread/source binding; attempts may change route but cannot change task content.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ThreadCommandBinding {
    pub source_mac_id: String,
    pub collaboration_workspace_id: String,
    pub thread_id: String,
    pub source_operation_id: String,
    pub source_message_id: String,
    pub payload: String,
}

#[derive(Debug, Serialize)]
pub struct StoredThreadCommand {
    pub binding: ThreadCommandBinding,
    pub fingerprint: String,
    pub receipt: MobileCommandReceipt,
}

impl MobileCommandStore {
    /// Background queue dispatch recovers the binding without trusting a new phone request.
    pub fn queued_thread_binding(
        &self,
        request: &NewMobileCommand,
    ) -> StorageResult<Option<ThreadCommandBinding>> {
        let connection = open_app_connection(&self.path)?;
        queue::checked(&connection, request)?;
        read_binding(&connection, &request.device_id, &request.operation_id)
    }

    /// Only an earlier, Provider-confirmed message for this same original task can advance its turn.
    /// A request not yet queued uses the latest existing predecessor; a saved request excludes itself
    /// and every later queue entry. No baseline/file snapshots are deserialized here.
    pub fn latest_thread_message(
        &self,
        request: &NewMobileCommand,
        binding: &ThreadCommandBinding,
        parent_operation: &str,
        task_ref: &str,
    ) -> StorageResult<Option<QueuedMessage>> {
        validate(request)?;
        binding.validate()?;
        crate::mobile_access_store::validate_id(parent_operation)?;
        if task_ref.is_empty() || task_ref.len() > 160 {
            return Err(invalid("invalid parent task reference"));
        }
        let connection = open_app_connection(&self.path)?;
        // ponytail: scan this device/project's bound queue history on the background owner;
        // add an indexed parent column only if measured history size makes this lookup costly.
        let operation: Option<String> = connection.query_row(
            "SELECT q.operation_id FROM mobile_message_queue q
             JOIN mobile_commands c USING(device_id,operation_id)
             JOIN mobile_thread_commands t USING(device_id,operation_id)
             WHERE c.device_id NOT GLOB 'connector:*' AND q.device_id=?1 AND c.workspace_id=?2 AND c.route_id=?3 AND q.state='sent'
             AND json_extract(t.binding_json,'$.source_mac_id')=?4
             AND json_extract(t.binding_json,'$.collaboration_workspace_id')=?5
             AND json_extract(t.binding_json,'$.thread_id')=?6
             AND json_extract(json_extract(t.binding_json,'$.payload'),'$.parent_operation_id')=?7
             AND json_extract(json_extract(t.binding_json,'$.payload'),'$.task_ref')=?8
             AND q.sequence < coalesce((SELECT sequence FROM mobile_message_queue WHERE device_id=?1 AND operation_id=?9),9223372036854775807)
             ORDER BY q.sequence DESC LIMIT 1",
            params![request.device_id,request.workspace_id,request.route_id,binding.source_mac_id,
                binding.collaboration_workspace_id,binding.thread_id,parent_operation,task_ref,request.operation_id],
            |row| row.get(0),
        ).optional()?;
        operation
            .map(|operation| {
                queue::read_queue(&connection, &request.device_id, &operation)
                    .and_then(|queue| queue.ok_or_else(|| invalid("missing prior follow-up queue")))
            })
            .transpose()
    }

    /// Binding and the existing command reservation commit together, before any foreground work.
    pub fn reserve_thread_command(
        &self,
        request: &NewMobileCommand,
        binding: &ThreadCommandBinding,
    ) -> StorageResult<MobileCommandReservation> {
        self.reserve_thread_binding(request, binding, None)
    }

    /// A follow-up reuses the durable message queue; no unbound input can survive a failed commit.
    pub fn enqueue_thread_message(
        &self,
        request: &NewMobileCommand,
        binding: &ThreadCommandBinding,
        message: &NewQueuedMessage,
    ) -> StorageResult<MobileCommandReservation> {
        self.reserve_thread_binding(request, binding, Some(message))
    }

    fn reserve_thread_binding(
        &self,
        request: &NewMobileCommand,
        binding: &ThreadCommandBinding,
        message: Option<&NewQueuedMessage>,
    ) -> StorageResult<MobileCommandReservation> {
        validate(request)?;
        binding.validate()?;
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = read_binding(&tx, &request.device_id, &request.operation_id)?;
        if let Some(existing) = existing {
            if &existing != binding {
                return Ok(MobileCommandReservation::Conflict);
            }
            let (mut receipt, owner, hash) = read(&tx, &request.device_id, &request.operation_id)?
                .ok_or_else(|| invalid("thread command missing"))?;
            if receipt.workspace_id != request.workspace_id || hash != request.request_hash {
                return Ok(MobileCommandReservation::Conflict);
            }
            match (
                queue::read_queue(&tx, &request.device_id, &request.operation_id)?,
                message,
            ) {
                (None, None) => {}
                (Some(queued), Some(message)) if queued.message == *message => {}
                _ => return Ok(MobileCommandReservation::Conflict),
            }
            // Only this explicitly bound protocol excludes route from idempotency.
            // Never give an existing operation a fresh lease or change its original route.
            recover(&mut receipt, &owner, &request.owner_id);
            return Ok(MobileCommandReservation::Existing(Box::new(receipt)));
        }
        if read(&tx, &request.device_id, &request.operation_id)?.is_some() {
            return Ok(MobileCommandReservation::Conflict);
        }
        let source_exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM mobile_thread_messages m
             JOIN mobile_collaboration_threads t ON t.device_id=m.device_id AND t.thread_id=m.thread_id
             WHERE m.device_id=?1 AND m.operation_id=?2 AND m.message_id=?3 AND m.thread_id=?4
             AND t.collaboration_workspace_id=?5 AND t.source_mac_id=?6)",
            params![request.device_id, binding.source_operation_id, binding.source_message_id,
                binding.thread_id, binding.collaboration_workspace_id, binding.source_mac_id],
            |row| row.get(0),
        )?;
        if !source_exists {
            return Err(invalid("thread source missing"));
        }
        let reservation = match message {
            Some(message) => queue::enqueue_on(&tx, request, message)?,
            None => reserve_on(&tx, request)?,
        };
        tx.execute("INSERT INTO mobile_thread_commands(device_id,operation_id,source_operation_id,binding_json)
            VALUES(?1,?2,?3,?4)", params![request.device_id, request.operation_id,
                binding.source_operation_id, serde_json::to_string(binding)?])?;
        tx.commit()?;
        Ok(reservation)
    }

    pub fn thread_command(
        &self,
        device: &str,
        mac: &str,
        workspace: &str,
        thread: &str,
        operation: &str,
        current_owner: &str,
    ) -> StorageResult<Option<StoredThreadCommand>> {
        for id in [device, mac, workspace, thread, operation, current_owner] {
            crate::mobile_access_store::validate_id(id)?;
        }
        let connection = open_app_connection(&self.path)?;
        let Some(binding) = read_binding(&connection, device, operation)? else {
            return Ok(None);
        };
        if binding.source_mac_id != mac
            || binding.collaboration_workspace_id != workspace
            || binding.thread_id != thread
        {
            return Ok(None);
        }
        let (mut receipt, owner, fingerprint) = read(&connection, device, operation)?
            .ok_or_else(|| invalid("thread command missing"))?;
        recover(&mut receipt, &owner, current_owner);
        Ok(Some(StoredThreadCommand {
            binding,
            fingerprint,
            receipt,
        }))
    }
}

impl ThreadCommandBinding {
    fn validate(&self) -> StorageResult<()> {
        for id in [
            &self.source_mac_id,
            &self.collaboration_workspace_id,
            &self.thread_id,
            &self.source_operation_id,
            &self.source_message_id,
        ] {
            crate::mobile_access_store::validate_id(id)?;
        }
        // 128 KiB leaves 128 KiB of a 256 KiB relay frame for receipt/routing metadata and escaping.
        if self.payload.len() > 128 * 1024
            || !serde_json::from_str::<Value>(&self.payload)?.is_object()
        {
            return Err(invalid("invalid thread command payload"));
        }
        Ok(())
    }
}

pub(super) fn read_binding(
    connection: &Connection,
    device: &str,
    operation: &str,
) -> StorageResult<Option<ThreadCommandBinding>> {
    let json: Option<String> = connection.query_row(
        "SELECT binding_json FROM mobile_thread_commands WHERE device_id NOT GLOB 'connector:*' AND device_id=?1 AND operation_id=?2",
        params![device, operation], |row| row.get(0)).optional()?;
    json.map(|value| serde_json::from_str(&value).map_err(StorageError::from))
        .transpose()
}

pub(super) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS mobile_thread_commands(
        device_id TEXT NOT NULL,operation_id TEXT NOT NULL,source_operation_id TEXT NOT NULL,binding_json TEXT NOT NULL,
        PRIMARY KEY(device_id,operation_id),
        FOREIGN KEY(device_id,operation_id) REFERENCES mobile_commands(device_id,operation_id),
        FOREIGN KEY(device_id,source_operation_id) REFERENCES mobile_thread_messages(device_id,operation_id));")?;
    Ok(())
}
