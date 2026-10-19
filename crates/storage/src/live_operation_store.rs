//! A reservation is durable before a tool runs. Unresolved work is never automatically reclaimed.
use crate::{migrations::open_app_connection, StorageError, StorageResult};
use agent::{AdapterCommand, AdapterError, AdapterResult};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf};

mod task_facts;
pub use task_facts::{LiveTaskFact, LiveTaskWatch};

const MAX_JSON_BYTES: usize = 64 * 1024;
type Outcome = Result<AdapterResult, AdapterError>;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LiveOperationKey {
    pub project_id: String,
    pub intent_id: String,
    pub operation_id: String,
}

/// The host supplies an observed intent identity, never just a new model delegation ID.
pub struct LiveOperationRequest<'a> {
    pub project_id: &'a str,
    pub intent_id: &'a str,
    pub operation_id: &'a str,
    pub owner_id: &'a str,
    pub command: &'a AdapterCommand,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum LiveOperationState {
    Pending,
    Recorded,
    OutcomeUnknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LiveOperationReceipt {
    pub key: LiveOperationKey,
    pub command: Value,
    pub state: LiveOperationState,
    /// An accepted task receipt is recorded, but does not mean the task completed.
    pub outcome: Option<Outcome>,
}

#[derive(Debug)]
pub struct LiveOperationLease {
    key: LiveOperationKey,
    owner_id: String,
    fingerprint: String,
    claim_token: String,
}

#[derive(Debug)]
pub enum LiveOperationReservation {
    Claimed(LiveOperationLease),
    Existing(Box<LiveOperationReceipt>),
}

#[derive(Clone)]
pub struct LiveOperationStore {
    path: PathBuf,
}

struct Stored {
    owner: String,
    fingerprint: String,
    command: String,
    outcome: Option<String>,
    claim_token: String,
}

impl LiveOperationStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Read-only reconciliation never renews a lease or changes the immutable original outcome.
    pub fn recent(&self, project: &str, limit: usize) -> StorageResult<Vec<LiveOperationReceipt>> {
        validate_project(project)?;
        let connection = open_app_connection(&self.path)?;
        let mut statement = connection.prepare("SELECT intent_id,operation_id,owner_id,fingerprint,command_json,outcome_json,claim_token FROM live_tool_operations WHERE project_id=?1 ORDER BY created_at_unix DESC,rowid DESC LIMIT ?2")?;
        let rows = statement.query_map(params![project, limit.min(100) as i64], |row| {
            Ok((
                LiveOperationKey {
                    project_id: project.into(),
                    intent_id: row.get(0)?,
                    operation_id: row.get(1)?,
                },
                Stored {
                    owner: row.get(2)?,
                    fingerprint: row.get(3)?,
                    command: row.get(4)?,
                    outcome: row.get(5)?,
                    claim_token: row.get(6)?,
                },
            ))
        })?;
        rows.map(|row| {
            let (key, stored) = row?;
            receipt(key, stored, "")
        })
        .collect()
    }

    pub fn reserve(
        &self,
        request: &LiveOperationRequest<'_>,
    ) -> StorageResult<LiveOperationReservation> {
        validate_project(request.project_id)?;
        for value in [request.intent_id, request.operation_id, request.owner_id] {
            validate_id(value)?;
        }
        let command = serde_json::to_string(&canonical(serde_json::to_value(request.command)?))?;
        if command.len() > MAX_JSON_BYTES {
            return Err(invalid("command too large"));
        }
        let fingerprint = format!("{:x}", Sha256::digest(command.as_bytes()));
        let key = LiveOperationKey {
            project_id: request.project_id.into(),
            intent_id: request.intent_id.into(),
            operation_id: request.operation_id.into(),
        };
        let mut connection = open_app_connection(&self.path)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result = if let Some(stored) = read(&transaction, &key)? {
            if stored.fingerprint != fingerprint || stored.command != command {
                return Err(invalid(
                    "operation identity conflicts with its recorded command",
                ));
            }
            LiveOperationReservation::Existing(Box::new(receipt(key, stored, request.owner_id)?))
        } else {
            let claim_token: String =
                transaction.query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))?;
            transaction.execute(
                "INSERT INTO live_tool_operations(project_id,intent_id,operation_id,owner_id,fingerprint,command_json,created_at_unix,claim_token) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![key.project_id,key.intent_id,key.operation_id,request.owner_id,fingerprint,command,crate::current_unix_timestamp()?,claim_token],
            )?;
            LiveOperationReservation::Claimed(LiveOperationLease {
                key,
                owner_id: request.owner_id.into(),
                fingerprint,
                claim_token,
            })
        };
        transaction.commit()?;
        Ok(result)
    }

    /// Persist only the actual Adapter outcome. Repeating persistence cannot change a known receipt.
    pub fn record(&self, lease: &LiveOperationLease, outcome: &Outcome) -> StorageResult<()> {
        let encoded = serde_json::to_string(outcome)?;
        if encoded.len() > MAX_JSON_BYTES {
            return Err(invalid(
                "outcome too large; reconcile before retrying the operation",
            ));
        }
        let mut connection = open_app_connection(&self.path)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let stored =
            read(&transaction, &lease.key)?.ok_or_else(|| invalid("reservation missing"))?;
        if stored.owner != lease.owner_id
            || stored.fingerprint != lease.fingerprint
            || stored.claim_token != lease.claim_token
        {
            return Err(invalid("reservation does not belong to this execution"));
        }
        if let Some(previous) = stored.outcome {
            let previous: Outcome = serde_json::from_str(&previous)?;
            if previous != *outcome {
                return Err(invalid("recorded outcome is immutable"));
            }
        } else {
            transaction.execute(
                "UPDATE live_tool_operations SET outcome_json=?4 WHERE project_id=?1 AND intent_id=?2 AND operation_id=?3",
                params![lease.key.project_id,lease.key.intent_id,lease.key.operation_id,encoded],
            )?;
        }
        task_facts::register(&transaction, &lease.key, outcome)?;
        transaction.commit()?;
        Ok(())
    }
}

fn read(connection: &Connection, key: &LiveOperationKey) -> StorageResult<Option<Stored>> {
    Ok(connection.query_row(
        "SELECT owner_id,fingerprint,command_json,outcome_json,claim_token FROM live_tool_operations WHERE project_id=?1 AND intent_id=?2 AND operation_id=?3",
        params![key.project_id,key.intent_id,key.operation_id],
        |row| Ok(Stored { owner:row.get(0)?,fingerprint:row.get(1)?,command:row.get(2)?,outcome:row.get(3)?,claim_token:row.get(4)? }),
    ).optional()?)
}

fn receipt(
    key: LiveOperationKey,
    stored: Stored,
    current_owner: &str,
) -> StorageResult<LiveOperationReceipt> {
    let outcome: Option<Outcome> = stored
        .outcome
        .as_deref()
        .map(serde_json::from_str)
        .transpose()?;
    let state = match &outcome {
        Some(Ok(result)) if matches!(result.status.as_str(), "accepted" | "succeeded") => {
            LiveOperationState::Recorded
        }
        // A generic error can occur after a partial effect. Only host reconciliation can clear it.
        Some(_) => LiveOperationState::OutcomeUnknown,
        None if stored.owner == current_owner => LiveOperationState::Pending,
        None => LiveOperationState::OutcomeUnknown,
    };
    Ok(LiveOperationReceipt {
        key,
        command: serde_json::from_str(&stored.command)?,
        state,
        outcome,
    })
}

fn canonical(value: Value) -> Value {
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key, canonical(value)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(canonical).collect()),
        other => other,
    }
}

fn validate_project(value: &str) -> StorageResult<()> {
    if value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
        return Err(invalid("invalid project identity"));
    }
    Ok(())
}

fn validate_id(value: &str) -> StorageResult<()> {
    if value.is_empty()
        || value.len() > 512
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':' | b'.'))
    {
        return Err(invalid("invalid operation, intent or owner identity"));
    }
    Ok(())
}

fn invalid(reason: &str) -> StorageError {
    StorageError::InvalidLiveOperation(reason.into())
}

pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS live_tool_operations (
        project_id TEXT NOT NULL,intent_id TEXT NOT NULL,operation_id TEXT NOT NULL,
        owner_id TEXT NOT NULL,fingerprint TEXT NOT NULL,command_json TEXT NOT NULL,
        outcome_json TEXT,created_at_unix INTEGER NOT NULL,claim_token TEXT NOT NULL,
        PRIMARY KEY(project_id,intent_id,operation_id));",
    )?;
    task_facts::migrate(connection)
}
