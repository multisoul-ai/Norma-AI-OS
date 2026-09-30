use super::{invalid, MobileCommandStore};
use crate::{migrations::open_app_connection, StorageResult};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use serde_json::Value;

#[derive(Clone)]
pub struct RealtimeCall {
    pub device_id: String,
    pub call_id: String,
    pub turn_id: String,
    pub workspace_id: String,
    pub operation_id: String,
    pub request_hash: String,
}
pub enum RealtimeReservation {
    New,
    Existing(Value),
    Pending,
    Blocked(agent::AdapterError),
}
#[derive(Serialize)]
pub struct RealtimeOperation {
    pub sequence: u64,
    pub device_id: String,
    pub call_id: String,
    pub turn_id: String,
    pub workspace_id: String,
    pub operation_id: String,
    pub result: Option<Value>,
}
impl RealtimeCall {
    fn validate(&self) -> StorageResult<()> {
        for id in [
            &self.device_id,
            &self.call_id,
            &self.turn_id,
            &self.workspace_id,
            &self.operation_id,
        ] {
            crate::mobile_access_store::validate_id(id)?;
        }
        if self.request_hash.len() != 64
            || !self.request_hash.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err(invalid("invalid realtime fingerprint"));
        }
        Ok(())
    }
}

impl MobileCommandStore {
    /// Read a bounded page for the authenticated device/project. Recover only from an
    /// already-stored command receipt; this path cannot reserve or dispatch a command.
    pub fn realtime_operations(
        &self,
        device: &str,
        workspace: &str,
        after: u64,
        owner: &str,
    ) -> StorageResult<Vec<RealtimeOperation>> {
        crate::mobile_access_store::validate_id(device)?;
        crate::mobile_access_store::validate_id(workspace)?;
        if after > i64::MAX as u64 {
            return Err(invalid("invalid realtime cursor"));
        }
        let connection = open_app_connection(&self.path)?;
        let mut query = connection.prepare(
            "SELECT sequence,call_id,turn_id,operation_id,request_hash,result_json
            FROM mobile_realtime_calls WHERE device_id=?1 AND workspace_id=?2 AND sequence>?3
            ORDER BY sequence LIMIT 20",
        )?;
        let rows = query
            .query_map(params![device, workspace, after], |row| {
                Ok((
                    row.get::<_, u64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(query);
        drop(connection);
        rows.into_iter().map(|(sequence,call_id,turn_id,operation_id,request_hash,json)| {
            let call = RealtimeCall { device_id: device.into(), call_id: call_id.clone(), turn_id: turn_id.clone(),
                workspace_id: workspace.into(), operation_id: operation_id.clone(), request_hash };
            let mut result: Option<Value> = json.map(|json| serde_json::from_str(&json)).transpose()?;
            if result.as_ref().is_none_or(|value| value["status"] == "unknown") {
                if let Some(receipt) = self.get(device, &operation_id, workspace, owner)? {
                    let verified = match receipt.state.as_str() {
                        "succeeded" => receipt.result,
                        "rejected" => Some(serde_json::json!({"status":"rejected","message":receipt.error})),
                        _ => None,
                    };
                    if let Some(verified) = verified {
                        self.finish_realtime(&call, verified.clone())?;
                        result = Some(verified);
                    }
                }
            }
            Ok(RealtimeOperation { sequence, device_id: device.into(), call_id, turn_id,
                workspace_id: workspace.into(), operation_id, result })
        }).collect()
    }
    /// Atomic reservation is durable BEFORE tool parsing/execution. An unfinished row freezes
    /// the turn across connections/processes; only the original durable receipt can resolve it.
    pub fn reserve_realtime(&self, call: &RealtimeCall) -> StorageResult<RealtimeReservation> {
        call.validate()?;
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(result) = read(&tx, call)? {
            return Ok(match result {
                Some(value) => RealtimeReservation::Existing(value),
                None => RealtimeReservation::Pending,
            });
        }
        let mut budget = agent::ToolCallBudget::default();
        {
            let mut query = tx.prepare(
                "SELECT result_json FROM mobile_realtime_calls
                WHERE device_id=?1 AND call_id=?2 AND turn_id=?3 ORDER BY sequence",
            )?;
            let rows = query
                .query_map(params![call.device_id, call.call_id, call.turn_id], |row| {
                    row.get::<_, Option<String>>(0)
                })?;
            for row in rows {
                let Some(json) = row? else {
                    return Ok(RealtimeReservation::Blocked(agent::AdapterError::Failed(
                        "原操作结果待核对，禁止继续派工".into(),
                    )));
                };
                budget
                    .reserve()
                    .map_err(|_| invalid("invalid stored realtime budget"))?;
                let result: Value = serde_json::from_str(&json)?;
                budget.observe(
                    result["status"]
                        .as_str()
                        .ok_or_else(|| invalid("missing tool status"))?,
                );
            }
        }
        if let Err(error) = budget.reserve() {
            return Ok(RealtimeReservation::Blocked(error));
        }
        tx.execute(
            "INSERT INTO mobile_realtime_calls
            (device_id,call_id,turn_id,workspace_id,operation_id,request_hash)
            VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                call.device_id,
                call.call_id,
                call.turn_id,
                call.workspace_id,
                call.operation_id,
                call.request_hash
            ],
        )?;
        tx.commit()?;
        Ok(RealtimeReservation::New)
    }

    pub fn finish_realtime(&self, call: &RealtimeCall, result: Value) -> StorageResult<()> {
        call.validate()?;
        let json = serde_json::to_string(&result)?;
        // Same 4 KiB result bound as the underlying command receipt, not a second task database.
        if json.len() > 4096
            || !matches!(
                result["status"].as_str(),
                Some("accepted" | "succeeded" | "rejected" | "unknown")
            )
        {
            return Err(invalid("invalid realtime result"));
        }
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous = read(&tx, call)?.ok_or_else(|| invalid("realtime call not reserved"))?;
        if let Some(previous) = previous {
            // A later authenticated command receipt can settle an unknown result. Never mutate
            // any already-known result, including a rejection.
            if previous == result {
                return Ok(());
            }
            if previous["status"] != "unknown" {
                return Err(invalid("realtime result is immutable"));
            }
        }
        tx.execute("UPDATE mobile_realtime_calls SET result_json=?1 WHERE device_id=?2 AND operation_id=?3",
            params![json,call.device_id,call.operation_id])?;
        tx.commit()?;
        Ok(())
    }
}

fn read(connection: &Connection, call: &RealtimeCall) -> StorageResult<Option<Option<Value>>> {
    let row = connection
        .query_row(
            "SELECT call_id,turn_id,workspace_id,request_hash,result_json
        FROM mobile_realtime_calls WHERE device_id=?1 AND operation_id=?2",
            params![call.device_id, call.operation_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .optional()?;
    row.map(|(media, turn, workspace, hash, json)| {
        if media != call.call_id
            || turn != call.turn_id
            || workspace != call.workspace_id
            || hash != call.request_hash
        {
            return Err(invalid("realtime identity conflict"));
        }
        json.map(|json| serde_json::from_str(&json).map_err(Into::into))
            .transpose()
    })
    .transpose()
}

pub(super) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS mobile_realtime_calls (
        sequence INTEGER PRIMARY KEY AUTOINCREMENT,
        device_id TEXT NOT NULL, call_id TEXT NOT NULL, turn_id TEXT NOT NULL,
        workspace_id TEXT NOT NULL, operation_id TEXT NOT NULL, request_hash TEXT NOT NULL,
        result_json TEXT, UNIQUE(device_id,operation_id));
        CREATE INDEX IF NOT EXISTS idx_mobile_realtime_turn
            ON mobile_realtime_calls(device_id,call_id,turn_id,sequence);",
    )?;
    Ok(())
}
