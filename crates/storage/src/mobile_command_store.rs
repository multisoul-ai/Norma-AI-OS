use crate::{current_unix_timestamp, migrations::open_app_connection, StorageError, StorageResult};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;

mod delivery;
mod feedback;
pub(crate) use delivery::migrate as migrate_delivery_acceptances;
pub use delivery::DeliveryAcceptance;
pub(crate) use feedback::migrate as migrate_delivery_feedback;
pub use feedback::DeliveryFeedback;
mod queue;
mod todo;
pub use queue::{NewQueuedMessage, QueuedMessage};
pub use todo::{PlanTodoReservation, StoredTodoDecision, TodoDecisionRef};

#[derive(Clone, Debug)]
pub struct NewMobileCommand {
    pub device_id: String,
    pub operation_id: String,
    pub workspace_id: String,
    pub route_id: String,
    pub owner_id: String,
    pub request_hash: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct MobileCommandReceipt {
    pub device_id: String,
    pub operation_id: String,
    pub workspace_id: String,
    pub route_id: String,
    pub state: String,
    pub result: Option<Value>,
    pub error: Option<String>,
}

#[derive(Debug)]
pub enum MobileCommandReservation {
    New,
    Existing(Box<MobileCommandReceipt>),
    Conflict,
}

/// Each device owns its operation IDs. No recovery path replays a reserved side effect.
#[derive(Clone, Debug)]
pub struct MobileCommandStore {
    path: PathBuf,
}

impl MobileCommandStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn reserve(&self, request: &NewMobileCommand) -> StorageResult<MobileCommandReservation> {
        validate(request)?;
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result = reserve_on(&tx, request)?;
        tx.commit()?;
        Ok(result)
    }

    pub fn get(
        &self,
        device: &str,
        operation: &str,
        workspace: &str,
        current_owner: &str,
    ) -> StorageResult<Option<MobileCommandReceipt>> {
        crate::mobile_access_store::validate_id(device)?;
        crate::mobile_access_store::validate_id(operation)?;
        let connection = open_app_connection(&self.path)?;
        let Some((mut receipt, owner, _)) = read(&connection, device, operation)? else {
            return Ok(None);
        };
        if receipt.workspace_id != workspace {
            return Ok(None);
        }
        recover(&mut receipt, &owner, current_owner);
        Ok(Some(receipt))
    }

    pub fn finish(
        &self,
        request: &NewMobileCommand,
        outcome: Result<Value, String>,
    ) -> StorageResult<()> {
        validate(request)?;
        let (state, mut result, error) = match outcome {
            Ok(value) => ("succeeded", Some(value), None),
            Err(code) => {
                crate::mobile_access_store::validate_id(&code)?;
                (
                    if code == "outcome_unknown" {
                        "outcome_unknown"
                    } else {
                        "rejected"
                    },
                    None,
                    Some(code),
                )
            }
        };
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (receipt, owner, hash) = read(&tx, &request.device_id, &request.operation_id)?
            .ok_or_else(|| invalid("command not reserved"))?;
        if owner != request.owner_id
            || hash != request.request_hash
            || receipt.workspace_id != request.workspace_id
            || receipt.route_id != request.route_id
        {
            return Err(invalid("command identity mismatch"));
        }
        feedback::bind_result(&tx, request, &mut result)?;
        let result_json = result.as_ref().map(serde_json::to_string).transpose()?;
        // Receipt payloads are identities and stable codes, never full chats or diffs.
        if result_json.as_ref().is_some_and(|json| json.len() > 4096) {
            return Err(invalid("receipt too large"));
        }
        if receipt.state != "accepted" {
            return if receipt.state == state && receipt.result == result && receipt.error == error {
                Ok(())
            } else {
                Err(invalid("receipt is immutable"))
            };
        }
        tx.execute("UPDATE mobile_commands SET state=?1,result_json=?2,error=?3 WHERE device_id=?4 AND operation_id=?5",
            params![state, result_json, error, request.device_id, request.operation_id])?;
        tx.commit()?;
        Ok(())
    }
}

fn read(
    connection: &Connection,
    device: &str,
    operation: &str,
) -> StorageResult<Option<(MobileCommandReceipt, String, String)>> {
    let row = connection.query_row("SELECT workspace_id,route_id,owner_id,request_hash,state,result_json,error FROM mobile_commands WHERE device_id=?1 AND operation_id=?2",
        params![device,operation], |row| Ok((row.get::<_,String>(0)?, row.get::<_,String>(1)?, row.get::<_,String>(2)?, row.get::<_,String>(3)?, row.get::<_,String>(4)?, row.get::<_,Option<String>>(5)?, row.get::<_,Option<String>>(6)?))).optional()?;
    row.map(
        |(workspace_id, route_id, owner, hash, state, json, error)| {
            Ok((
                MobileCommandReceipt {
                    device_id: device.into(),
                    operation_id: operation.into(),
                    workspace_id,
                    route_id,
                    state,
                    result: json.map(|value| serde_json::from_str(&value)).transpose()?,
                    error,
                },
                owner,
                hash,
            ))
        },
    )
    .transpose()
}

fn recover(receipt: &mut MobileCommandReceipt, owner: &str, current: &str) {
    if receipt.state == "accepted" && owner != current {
        receipt.state = "outcome_unknown".into();
        receipt.error = Some("outcome_unknown".into());
    }
}

fn validate(request: &NewMobileCommand) -> StorageResult<()> {
    for id in [
        &request.device_id,
        &request.operation_id,
        &request.workspace_id,
        &request.route_id,
        &request.owner_id,
    ] {
        crate::mobile_access_store::validate_id(id)?;
    }
    if request.request_hash.len() != 64
        || !request.request_hash.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err(invalid("invalid fingerprint"));
    }
    Ok(())
}

fn invalid(message: &str) -> StorageError {
    StorageError::InvalidMobileCommand(message.into())
}

fn reserve_on(
    connection: &Connection,
    request: &NewMobileCommand,
) -> StorageResult<MobileCommandReservation> {
    if let Some((mut receipt, owner, hash)) =
        read(connection, &request.device_id, &request.operation_id)?
    {
        if hash != request.request_hash
            || receipt.workspace_id != request.workspace_id
            || receipt.route_id != request.route_id
        {
            return Ok(MobileCommandReservation::Conflict);
        }
        recover(&mut receipt, &owner, &request.owner_id);
        return Ok(MobileCommandReservation::Existing(Box::new(receipt)));
    }
    connection.execute("INSERT INTO mobile_commands
            (device_id,operation_id,workspace_id,route_id,owner_id,request_hash,state,created_at_unix)
            VALUES (?1,?2,?3,?4,?5,?6,'accepted',?7)", params![request.device_id, request.operation_id,
            request.workspace_id, request.route_id, request.owner_id, request.request_hash, current_unix_timestamp()?])?;
    Ok(MobileCommandReservation::New)
}

/// Mobile command and FIFO tables share one migration owner.
pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch(r#"
        CREATE TABLE IF NOT EXISTS mobile_commands (
            device_id TEXT NOT NULL,
            operation_id TEXT NOT NULL,
            workspace_id TEXT NOT NULL,
            route_id TEXT NOT NULL,
            owner_id TEXT NOT NULL,
            request_hash TEXT NOT NULL,
            state TEXT NOT NULL CHECK(state IN ('accepted','succeeded','rejected','outcome_unknown')),
            result_json TEXT,
            error TEXT,
            created_at_unix INTEGER NOT NULL,
            PRIMARY KEY(device_id,operation_id)
        );

        CREATE TABLE IF NOT EXISTS mobile_message_queue (
            sequence INTEGER PRIMARY KEY AUTOINCREMENT,
            device_id TEXT NOT NULL,
            operation_id TEXT NOT NULL,
            node_id INTEGER NOT NULL CHECK(node_id > 0),
            profile TEXT NOT NULL CHECK(profile IN ('Codex','Claude')),
            run_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            grant_revision INTEGER NOT NULL CHECK(grant_revision > 0),
            message TEXT NOT NULL,
            state TEXT NOT NULL CHECK(state IN ('queued','dispatching','sent','cancelled','outcome_unknown')),
            result_json TEXT,
            error TEXT,
            UNIQUE(device_id,operation_id),
            FOREIGN KEY(device_id,operation_id) REFERENCES mobile_commands(device_id,operation_id)
        );
        CREATE INDEX IF NOT EXISTS idx_mobile_queue_state_sequence ON mobile_message_queue(state,sequence);

    "#)?;
    Ok(())
}
