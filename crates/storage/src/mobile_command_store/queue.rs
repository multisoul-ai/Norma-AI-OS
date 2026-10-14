//! Durable mobile messages; the caller revalidates authorization and the live session before claim.
use super::*;
use serde_json::json;

#[derive(Clone, Debug, PartialEq)]
pub struct NewQueuedMessage {
    pub node_id: u64,
    pub profile: String,
    pub run_id: String,
    pub session_id: String,
    pub grant_revision: u64,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct QueuedMessage {
    pub request: NewMobileCommand,
    pub message: NewQueuedMessage,
    pub state: String,
    pub result: Option<Value>,
    pub error: Option<String>,
}

const SELECT: &str = "SELECT c.device_id,c.operation_id,c.workspace_id,c.route_id,c.owner_id,c.request_hash,
    q.node_id,q.profile,q.run_id,q.session_id,q.grant_revision,q.message,q.state,q.result_json,q.error
    FROM mobile_message_queue q JOIN mobile_commands c USING(device_id,operation_id)";

impl MobileCommandStore {
    /// The enqueue acknowledgement and its full message commit together. No process starts here.
    pub fn enqueue_message(
        &self,
        request: &NewMobileCommand,
        message: &NewQueuedMessage,
    ) -> StorageResult<MobileCommandReservation> {
        validate(request)?;
        message.validate()?;
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result = enqueue_on(&tx, request, message)?;
        tx.commit()?;
        Ok(result)
    }

    /// Only this server owner's unclaimed messages are runnable; restart never adopts old routes.
    pub fn queued_messages(&self, owner: &str) -> StorageResult<Vec<QueuedMessage>> {
        crate::mobile_access_store::validate_id(owner)?;
        let connection = open_app_connection(&self.path)?;
        let mut statement = connection.prepare(&format!(
            "{SELECT} WHERE c.device_id NOT GLOB 'connector:*' AND c.owner_id=?1 AND q.state='queued' ORDER BY q.sequence LIMIT 128"
        ))?;
        let rows = statement.query_map([owner], row)?;
        rows.map(|entry| decode(entry?)).collect()
    }

    pub fn queued_message(
        &self,
        device: &str,
        operation: &str,
        workspace: &str,
        owner: &str,
    ) -> StorageResult<Option<QueuedMessage>> {
        for value in [device, operation, workspace, owner] {
            crate::mobile_access_store::validate_id(value)?;
        }
        let connection = open_app_connection(&self.path)?;
        let Some(mut entry) = read_queue(&connection, device, operation)? else {
            return Ok(None);
        };
        if entry.request.workspace_id != workspace {
            return Ok(None);
        }
        recover_owner(&mut entry, owner);
        Ok(Some(entry))
    }

    /// Includes every phone, and both waiting and claimed messages, so direct sends cannot overtake.
    pub fn has_pending_queued_message(
        &self,
        request: &NewMobileCommand,
        node: u64,
        run: &str,
    ) -> StorageResult<bool> {
        validate(request)?;
        crate::mobile_access_store::validate_id(run)?;
        let connection = open_app_connection(&self.path)?;
        Ok(connection.query_row("SELECT EXISTS(SELECT 1 FROM mobile_message_queue q JOIN mobile_commands c USING(device_id,operation_id)
            WHERE c.device_id NOT GLOB 'connector:*' AND c.owner_id=?1 AND c.workspace_id=?2 AND c.route_id=?3 AND q.node_id=?4 AND q.run_id=?5 AND q.state IN ('queued','dispatching'))",
            params![request.owner_id,request.workspace_id,request.route_id,node,run], |row| row.get(0))?)
    }

    /// One SQLite CAS competes with cancellation, and earlier messages on this same card win.
    /// Call only after a current authorized snapshot says input is ready; no UI-thread database IO.
    pub fn claim_queued_message(&self, request: &NewMobileCommand) -> StorageResult<bool> {
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let entry = checked(&tx, request)?;
        if entry.state != "queued" {
            return Ok(false);
        }
        let changed = tx.execute("UPDATE mobile_message_queue SET state='dispatching' WHERE device_id=?1 AND operation_id=?2 AND state='queued'
            AND NOT EXISTS (
                SELECT 1 FROM mobile_message_queue older JOIN mobile_commands c USING(device_id,operation_id)
                WHERE c.device_id NOT GLOB 'connector:*' AND c.owner_id=?3 AND c.workspace_id=?4 AND c.route_id=?5
                AND older.node_id=?6 AND older.run_id=?7 AND older.state IN ('queued','dispatching')
                AND older.sequence < (SELECT sequence FROM mobile_message_queue WHERE device_id=?1 AND operation_id=?2)
            )", params![request.device_id, request.operation_id, request.owner_id, request.workspace_id,
            request.route_id, entry.message.node_id, entry.message.run_id])?;
        tx.commit()?;
        Ok(changed == 1)
    }

    /// Once dispatching, cancellation cannot promise that no bytes reached the Provider.
    pub fn cancel_queued_message(&self, request: &NewMobileCommand) -> StorageResult<bool> {
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let entry = checked(&tx, request)?;
        if entry.state == "cancelled" {
            return Ok(true);
        }
        if entry.state != "queued" {
            return Ok(false);
        }
        tx.execute("UPDATE mobile_message_queue SET state='cancelled',error='cancelled' WHERE device_id=?1 AND operation_id=?2 AND state='queued'",
            params![request.device_id, request.operation_id])?;
        tx.commit()?;
        Ok(true)
    }

    /// Use only for an explicit pre-input rejection (busy/stale snapshot), never a timeout or lost reply.
    pub fn defer_queued_message(&self, request: &NewMobileCommand) -> StorageResult<()> {
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let entry = checked(&tx, request)?;
        if entry.state != "dispatching" {
            return Err(invalid("message was not claimed"));
        }
        tx.execute(
            "UPDATE mobile_message_queue SET state='queued' WHERE device_id=?1 AND operation_id=?2",
            params![request.device_id, request.operation_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// A known pre-input rejection is cancelled; ambiguous execution is outcome_unknown and never retried.
    pub fn finish_queued_message(
        &self,
        request: &NewMobileCommand,
        outcome: Result<Value, String>,
    ) -> StorageResult<()> {
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let entry = checked(&tx, request)?;
        let (state, result, error) = match outcome {
            Ok(value) => {
                if value["node_id"].as_u64() != Some(entry.message.node_id)
                    || value["run_id"].as_str() != Some(&entry.message.run_id)
                    || value["session_id"].as_str() != Some(&entry.message.session_id)
                    || value["turn_id"]
                        .as_str()
                        .is_none_or(|s| crate::mobile_access_store::validate_id(s).is_err())
                    || value.to_string().len() > 4096
                {
                    return Err(invalid("queue receipt identity mismatch"));
                }
                ("sent", Some(value), None)
            }
            Err(code) => {
                crate::mobile_access_store::validate_id(&code)?;
                (
                    if code == "outcome_unknown" {
                        "outcome_unknown"
                    } else {
                        "cancelled"
                    },
                    None,
                    Some(code),
                )
            }
        };
        if entry.state != "dispatching" {
            return if entry.state == state && entry.result == result && entry.error == error {
                Ok(())
            } else {
                Err(invalid("queue receipt is immutable or message not claimed"))
            };
        }
        tx.execute("UPDATE mobile_message_queue SET state=?1,result_json=?2,error=?3 WHERE device_id=?4 AND operation_id=?5",
            params![state, result.map(|v| v.to_string()), error, request.device_id, request.operation_id])?;
        tx.commit()?;
        Ok(())
    }
}

impl NewQueuedMessage {
    fn validate(&self) -> StorageResult<()> {
        for value in [&self.run_id, &self.session_id] {
            crate::mobile_access_store::validate_id(value)?;
        }
        if self.node_id == 0
            || self.node_id > i64::MAX as u64
            || self.grant_revision == 0
            || self.grant_revision > i64::MAX as u64
            || !crate::is_mobile_agent_profile(&self.profile)
            || self.text.trim().is_empty()
            || self.text.len() > 32 * 1024
            || self
                .text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err(invalid("invalid queued message"));
        }
        Ok(())
    }
}

pub(super) fn checked(
    connection: &Connection,
    request: &NewMobileCommand,
) -> StorageResult<QueuedMessage> {
    validate(request)?;
    let entry = read_queue(connection, &request.device_id, &request.operation_id)?
        .ok_or_else(|| invalid("queued message missing"))?;
    if entry.request.workspace_id != request.workspace_id
        || entry.request.route_id != request.route_id
        || entry.request.owner_id != request.owner_id
        || entry.request.request_hash != request.request_hash
    {
        return Err(invalid("queued message scope mismatch"));
    }
    Ok(entry)
}

pub(super) fn read_queue(
    connection: &Connection,
    device: &str,
    operation: &str,
) -> StorageResult<Option<QueuedMessage>> {
    connection
        .query_row(
            &format!("{SELECT} WHERE c.device_id NOT GLOB 'connector:*' AND c.device_id=?1 AND c.operation_id=?2"),
            params![device, operation],
            row,
        )
        .optional()?
        .map(decode)
        .transpose()
}

fn row(row: &rusqlite::Row<'_>) -> rusqlite::Result<(QueuedMessage, Option<String>)> {
    Ok((
        QueuedMessage {
            request: NewMobileCommand {
                device_id: row.get(0)?,
                operation_id: row.get(1)?,
                workspace_id: row.get(2)?,
                route_id: row.get(3)?,
                owner_id: row.get(4)?,
                request_hash: row.get(5)?,
            },
            message: NewQueuedMessage {
                node_id: row.get(6)?,
                profile: row.get(7)?,
                run_id: row.get(8)?,
                session_id: row.get(9)?,
                grant_revision: row.get(10)?,
                text: row.get(11)?,
            },
            state: row.get(12)?,
            result: None,
            error: row.get(14)?,
        },
        row.get(13)?,
    ))
}

fn decode((mut entry, json): (QueuedMessage, Option<String>)) -> StorageResult<QueuedMessage> {
    validate(&entry.request)?;
    entry.message.validate()?;
    entry.result = json.map(|value| serde_json::from_str(&value)).transpose()?;
    Ok(entry)
}

pub(super) fn enqueue_on(
    connection: &Connection,
    request: &NewMobileCommand,
    message: &NewQueuedMessage,
) -> StorageResult<MobileCommandReservation> {
    validate(request)?;
    message.validate()?;
    if let Some((receipt, _owner, hash)) =
        read(connection, &request.device_id, &request.operation_id)?
    {
        let queued = read_queue(connection, &request.device_id, &request.operation_id)?;
        if hash != request.request_hash
            || receipt.workspace_id != request.workspace_id
            || receipt.route_id != request.route_id
            || !queued.is_some_and(|entry| entry.message == *message)
        {
            return Ok(MobileCommandReservation::Conflict);
        }
        return Ok(MobileCommandReservation::Existing(Box::new(receipt)));
    }
    // 128 pending messages × 32KiB maximum = 4MiB of active message bodies per server owner.
    let count: u64 = connection.query_row("SELECT count(*) FROM mobile_message_queue q JOIN mobile_commands c USING(device_id,operation_id)
            WHERE c.device_id NOT GLOB 'connector:*' AND c.owner_id=?1 AND q.state IN ('queued','dispatching')", [&request.owner_id], |row| row.get(0))?;
    if count >= 128 {
        return Err(invalid("message queue full"));
    }
    let ack = json!({"node_id":message.node_id,"queue_id":request.operation_id,
            "run_id":message.run_id,"session_id":message.session_id,"status":"queued"})
    .to_string();
    connection.execute("INSERT INTO mobile_commands
            (device_id,operation_id,workspace_id,route_id,owner_id,request_hash,state,result_json,created_at_unix)
            VALUES (?1,?2,?3,?4,?5,?6,'succeeded',?7,?8)", params![request.device_id, request.operation_id,
            request.workspace_id, request.route_id, request.owner_id, request.request_hash, ack, current_unix_timestamp()?])?;
    connection.execute(
        "INSERT INTO mobile_message_queue
            (device_id,operation_id,node_id,profile,run_id,session_id,grant_revision,message,state)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'queued')",
        params![
            request.device_id,
            request.operation_id,
            message.node_id,
            message.profile,
            message.run_id,
            message.session_id,
            message.grant_revision,
            message.text
        ],
    )?;
    Ok(MobileCommandReservation::New)
}

pub(super) fn recover_owner(entry: &mut QueuedMessage, owner: &str) {
    if entry.request.owner_id != owner {
        match entry.state.as_str() {
            "queued" => {
                entry.state = "cancelled".into();
                entry.error = Some("owner_closed".into());
            }
            "dispatching" => {
                entry.state = "outcome_unknown".into();
                entry.error = Some("outcome_unknown".into());
            }
            _ => {}
        }
    }
}
