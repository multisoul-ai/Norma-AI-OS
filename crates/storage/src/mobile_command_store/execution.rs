use super::*;
use crate::{StoredAgentDispatchResult, TaskDeliverySummary};
use serde::Deserialize;

/// A bounded projection of the existing execution ledger, never a new execution queue.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ThreadTaskExecution {
    pub task_ref: String,
    pub node_id: u64,
    pub agent_definition_id: i64,
    pub state: String,
    pub tracking: String,
    pub error: Option<String>,
    pub result: Option<StoredAgentDispatchResult>,
    pub delivery: Option<TaskDeliverySummary>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ThreadTaskExecutionEvent {
    pub sequence: u64,
    pub event_id: String,
    pub execution: ThreadTaskExecution,
}

impl MobileCommandStore {
    /// The authenticated adapter passes the exact stored binding, not phone-supplied task refs.
    /// Re-reading the command and append/de-duplication share a transaction.
    pub fn record_thread_execution(
        &self,
        bound: &StoredThreadCommand,
        execution: &ThreadTaskExecution,
    ) -> StorageResult<ThreadTaskExecutionEvent> {
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        check_binding(&tx, bound)?;
        execution.validate(bound)?;
        validate_delivery(&tx, bound, execution)?;
        if let Some(previous) = latest(&tx, bound)? {
            if previous.execution == *execution {
                return Ok(previous);
            }
            if previous.execution.terminal() {
                if !execution.terminal() {
                    return Ok(previous);
                }
                let mut with_delivery = previous.execution.clone();
                with_delivery.delivery = execution.delivery.clone();
                if with_delivery != *execution {
                    return Err(invalid("terminal execution is immutable"));
                }
                // Delivery may arrive after the final reply, but never disappear or change turn.
                if previous.execution.delivery.is_some() {
                    if execution.delivery.is_none() {
                        return Ok(previous);
                    }
                    return Err(invalid("frozen delivery is immutable"));
                }
            }
        }
        tx.execute(
            "INSERT INTO mobile_thread_execution_events(device_id,operation_id,execution_json)
             VALUES(?1,?2,?3)",
            params![
                bound.receipt.device_id,
                bound.receipt.operation_id,
                serde_json::to_string(execution)?
            ],
        )?;
        let event = latest(&tx, bound)?.ok_or_else(|| invalid("execution event missing"))?;
        tx.commit()?;
        Ok(event)
    }

    pub fn thread_execution(
        &self,
        bound: &StoredThreadCommand,
    ) -> StorageResult<Option<ThreadTaskExecutionEvent>> {
        let connection = open_app_connection(&self.path)?;
        check_binding(&connection, bound)?;
        latest(&connection, bound)
    }
}

impl ThreadTaskExecution {
    fn terminal(&self) -> bool {
        matches!(
            self.state.as_str(),
            "done" | "failed" | "cancelled" | "interrupted"
        )
    }

    fn validate(&self, bound: &StoredThreadCommand) -> StorageResult<()> {
        let accepted = bound
            .receipt
            .result
            .as_ref()
            .ok_or_else(|| invalid("dispatch missing"))?;
        let payload: Value = serde_json::from_str(&bound.binding.payload)?;
        let identity = self
            .task_ref
            .split_once(':')
            .ok_or_else(|| invalid("invalid task ref"))?;
        crate::mobile_access_store::validate_id(identity.0)?;
        if self.task_ref.len() > 160
            || accepted["task_ref"].as_str() != Some(self.task_ref.as_str())
            || accepted["node_id"].as_u64() != Some(self.node_id)
            || accepted["task_id"].as_u64().map(|id| id.to_string()).as_deref() != Some(identity.1)
            || accepted["task_id"].as_u64().is_none_or(|id| id == 0 || id > i64::MAX as u64)
            || self.node_id == 0 || self.node_id > i64::MAX as u64
            || payload["agent_definition_id"].as_i64() != Some(self.agent_definition_id)
            || self.agent_definition_id <= 0
            || !matches!(self.state.as_str(), "accepted" | "submitted" | "queued" | "untracked" | "done" | "failed" | "cancel_requested" | "cancelled" | "interrupted")
            || !matches!(self.tracking.as_str(), "managed" | "untracked")
            || self.error.as_ref().is_some_and(|text| text.len() > 4096)
            || (self.result.is_some() && !self.terminal())
            || self.result.as_ref().and_then(|result| result.text.as_ref()).is_some_and(|text| text.len() > 16 * 1024)
            // 16KiB text may need 6x JSON escaping; leave room for the bound request in a 256KiB frame.
            || serde_json::to_vec(self)?.len() > 112 * 1024
        {
            return Err(invalid("execution does not match original dispatch"));
        }
        if let Some(delivery) = &self.delivery {
            delivery.outcome.key.validate()?;
            if delivery.workspace_id != bound.receipt.workspace_id
                || delivery.outcome.key.node_id != self.node_id
            {
                return Err(invalid("execution delivery scope mismatch"));
            }
        }
        Ok(())
    }
}

fn check_binding(connection: &Connection, bound: &StoredThreadCommand) -> StorageResult<()> {
    let binding = super::thread::read_binding(
        connection,
        &bound.receipt.device_id,
        &bound.receipt.operation_id,
    )?;
    let saved = read(
        connection,
        &bound.receipt.device_id,
        &bound.receipt.operation_id,
    )?;
    if binding.as_ref() != Some(&bound.binding)
        || saved.is_none_or(|(receipt, _, fingerprint)| {
            receipt != bound.receipt || fingerprint != bound.fingerprint
        })
        || bound.receipt.state != "succeeded"
        || bound
            .receipt
            .result
            .as_ref()
            .is_none_or(|result| result["status"] != "accepted")
    {
        return Err(invalid("execution requires original accepted thread task"));
    }
    Ok(())
}

fn validate_delivery(
    connection: &Connection,
    bound: &StoredThreadCommand,
    execution: &ThreadTaskExecution,
) -> StorageResult<()> {
    if let Some(delivery) = &execution.delivery {
        let original = crate::MobileTaskDeliveryStore::for_command_on(
            connection,
            &bound.receipt.device_id,
            &bound.receipt.operation_id,
            &bound.receipt.workspace_id,
        )?;
        if original.as_ref() != Some(delivery) {
            return Err(invalid(
                "execution delivery does not belong to original command",
            ));
        }
    }
    Ok(())
}

fn latest(
    connection: &Connection,
    bound: &StoredThreadCommand,
) -> StorageResult<Option<ThreadTaskExecutionEvent>> {
    let saved: Option<(u64, String)> = connection
        .query_row(
            "SELECT sequence,execution_json FROM mobile_thread_execution_events
         WHERE device_id=?1 AND operation_id=?2 ORDER BY sequence DESC LIMIT 1",
            params![bound.receipt.device_id, bound.receipt.operation_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    saved
        .map(|(sequence, json)| {
            let execution: ThreadTaskExecution = serde_json::from_str(&json)?;
            execution.validate(bound)?;
            validate_delivery(connection, bound, &execution)?;
            Ok(ThreadTaskExecutionEvent {
                sequence,
                event_id: format!("{}:execution:{sequence}", bound.binding.source_mac_id),
                execution,
            })
        })
        .transpose()
}

pub(super) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS mobile_thread_execution_events(
        sequence INTEGER PRIMARY KEY AUTOINCREMENT,device_id TEXT NOT NULL,operation_id TEXT NOT NULL,execution_json TEXT NOT NULL,
        FOREIGN KEY(device_id,operation_id) REFERENCES mobile_thread_commands(device_id,operation_id));
        CREATE INDEX IF NOT EXISTS mobile_thread_execution_scope ON mobile_thread_execution_events(device_id,operation_id,sequence);")?;
    Ok(())
}
