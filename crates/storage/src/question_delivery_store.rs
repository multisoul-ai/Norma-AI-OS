//! Durable intent precedes provider IO. An existing intent is never a reusable send lease.
use crate::{migrations::open_app_connection, StorageError, StorageResult};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Debug)]
pub struct QuestionDeliveryDraft {
    pub delivery_id: String,
    /// Consumer-neutral wire values; the host validates provider-specific semantics before reserve.
    pub target: Value,
    pub question: Value,
    pub answers: Value,
}

#[derive(Clone, Debug)]
pub struct QuestionDeliveryLease {
    key: String,
    delivery_id: String,
    token: String,
}

impl QuestionDeliveryLease {
    pub fn delivery_id(&self) -> &str {
        &self.delivery_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionDeliveryState {
    Unknown,
    Answered,
    AnsweredElsewhere,
    ConsumedUnattributed,
    Cancelled,
    Expired,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QuestionDeliveryReceipt {
    pub delivery_id: String,
    pub target: Value,
    pub state: QuestionDeliveryState,
    pub evidence: Option<String>,
}

pub enum QuestionDeliveryReservation {
    Claimed(QuestionDeliveryLease),
    Existing(QuestionDeliveryReceipt),
}

#[derive(Clone)]
pub struct QuestionDeliveryStore {
    path: PathBuf,
}

struct Stored {
    delivery: String,
    target: String,
    fingerprint: String,
    token: String,
    evidence: Option<String>,
    state: Option<String>,
}

impl QuestionDeliveryStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn reserve(
        &self,
        draft: &QuestionDeliveryDraft,
    ) -> StorageResult<QuestionDeliveryReservation> {
        if draft.delivery_id.trim().is_empty() || draft.delivery_id.len() > 256 {
            return Err(invalid("invalid delivery identity"));
        }
        let (target, key) = target_key(&draft.target)?;
        let question = encode(&draft.question)?;
        let answers = encode(&draft.answers)?;
        let fingerprint = digest(&format!("{question}\n{answers}"));
        let mut connection = open_app_connection(&self.path)?;
        connection.busy_timeout(std::time::Duration::from_secs(2))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result = if let Some(stored) = read(&transaction, &key)? {
            if stored.target != target || stored.fingerprint != fingerprint {
                return Err(invalid(
                    "question identity conflicts with recorded question or answers",
                ));
            }
            QuestionDeliveryReservation::Existing(receipt(stored)?)
        } else {
            let token: String =
                transaction.query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))?;
            transaction.execute(
                "INSERT INTO question_deliveries(target_key,target_json,delivery_id,fingerprint,question_json,answers_json,claim_token,created_at_unix) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![key,target,draft.delivery_id,fingerprint,question,answers,token,crate::current_unix_timestamp()?],
            )?;
            QuestionDeliveryReservation::Claimed(QuestionDeliveryLease {
                key,
                delivery_id: draft.delivery_id.clone(),
                token,
            })
        };
        transaction.commit()?;
        Ok(result)
    }

    pub fn get(&self, target: &Value) -> StorageResult<Option<QuestionDeliveryReceipt>> {
        let (encoded, key) = target_key(target)?;
        let connection = open_app_connection(&self.path)?;
        let Some(stored) = read(&connection, &key)? else {
            return Ok(None);
        };
        if stored.target != encoded {
            return Err(invalid("question identity collision"));
        }
        receipt(stored).map(Some)
    }

    /// A provider's correlated consumption evidence is required. A successful write is not enough.
    pub fn confirm(&self, lease: &QuestionDeliveryLease, evidence: &str) -> StorageResult<()> {
        self.settle(lease, QuestionDeliveryState::Answered, evidence)
    }

    /// Close one original intent without assigning successful delivery to an unknown sender.
    pub fn settle(
        &self,
        lease: &QuestionDeliveryLease,
        state: QuestionDeliveryState,
        evidence: &str,
    ) -> StorageResult<()> {
        if state == QuestionDeliveryState::Unknown
            || evidence.trim().is_empty()
            || evidence.len() > 32768
        {
            return Err(invalid("invalid consumption evidence"));
        }
        let mut connection = open_app_connection(&self.path)?;
        connection.busy_timeout(std::time::Duration::from_secs(2))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let stored =
            read(&transaction, &lease.key)?.ok_or_else(|| invalid("delivery intent missing"))?;
        if stored.delivery != lease.delivery_id || stored.token != lease.token {
            return Err(invalid("delivery lease mismatch"));
        }
        if let Some(previous) = &stored.evidence {
            if previous != evidence || stored_state(&stored)? != state {
                return Err(invalid("consumption evidence is immutable"));
            }
        } else {
            transaction.execute(
                "UPDATE question_deliveries SET evidence=?2,outcome=?3 WHERE target_key=?1",
                params![lease.key, evidence, serde_json::to_string(&state)?],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }
}

fn read(connection: &Connection, key: &str) -> StorageResult<Option<Stored>> {
    Ok(connection.query_row("SELECT delivery_id,target_json,fingerprint,claim_token,evidence,outcome FROM question_deliveries WHERE target_key=?1", [key], |row| {
        Ok(Stored {delivery:row.get(0)?,target:row.get(1)?,fingerprint:row.get(2)?,token:row.get(3)?,evidence:row.get(4)?,state:row.get(5)?})
    }).optional()?)
}

fn receipt(stored: Stored) -> StorageResult<QuestionDeliveryReceipt> {
    let state = stored_state(&stored)?;
    Ok(QuestionDeliveryReceipt {
        delivery_id: stored.delivery,
        target: serde_json::from_str(&stored.target)?,
        state,
        evidence: stored.evidence,
    })
}

fn stored_state(stored: &Stored) -> StorageResult<QuestionDeliveryState> {
    match &stored.state {
        Some(state) => Ok(serde_json::from_str(state)?),
        None if stored.evidence.is_some() => Ok(QuestionDeliveryState::Answered),
        None => Ok(QuestionDeliveryState::Unknown),
    }
}

fn target_key(target: &Value) -> StorageResult<(String, String)> {
    let fields = target
        .as_object()
        .ok_or_else(|| invalid("invalid question target"))?;
    for field in [
        "workspace_id",
        "project_id",
        "provider",
        "run_id",
        "connection_id",
        "session_id",
    ] {
        if fields
            .get(field)
            .and_then(Value::as_str)
            .is_none_or(|id| id.trim().is_empty() || id.len() > 4096)
        {
            return Err(invalid("missing question identity"));
        }
    }
    if target["node_id"].as_u64().is_none_or(|id| id == 0)
        || target["revision"].as_u64().is_none_or(|id| id == 0)
        || !(target["request_id"].as_i64().is_some()
            || target["request_id"]
                .as_str()
                .is_some_and(|id| !id.trim().is_empty() && id.len() <= 4096))
    {
        return Err(invalid("invalid request identity"));
    }
    let encoded = encode(target)?;
    let key = digest(&encoded);
    Ok((encoded, key))
}

fn encode(value: &Value) -> StorageResult<String> {
    let encoded = serde_json::to_string(&canonical(value.clone()))?;
    if encoded.len() > 1_048_576 {
        return Err(invalid("question delivery payload too large"));
    }
    Ok(encoded)
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
        value => value,
    }
}

fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn invalid(reason: &str) -> StorageError {
    StorageError::InvalidQuestionDelivery(reason.into())
}

pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS question_deliveries (
        target_key TEXT PRIMARY KEY NOT NULL,target_json TEXT NOT NULL,delivery_id TEXT UNIQUE NOT NULL,
        fingerprint TEXT NOT NULL,question_json TEXT NOT NULL,answers_json TEXT NOT NULL,
        claim_token TEXT NOT NULL,evidence TEXT,outcome TEXT,created_at_unix INTEGER NOT NULL);")?;
    let has_outcome = connection
        .prepare("PRAGMA table_info(question_deliveries)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|name| name == "outcome");
    if !has_outcome {
        // Another background reader can migrate this legacy table after our first check.
        // Serialize only the upgrade and recheck under its writer lock.
        let transaction =
            rusqlite::Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
        crate::migrations::ensure_column(
            &transaction,
            "question_deliveries",
            "outcome",
            "outcome TEXT",
        )?;
        transaction.commit()?;
    }
    Ok(())
}
