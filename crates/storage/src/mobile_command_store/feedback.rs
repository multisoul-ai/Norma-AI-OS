use super::*;
use crate::{MobileTurnKey, TaskDeliveryContent};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeliveryFeedback {
    pub key: MobileTurnKey,
    pub fingerprint: String,
    pub text: String,
}
impl DeliveryFeedback {
    pub fn validate(&self) -> StorageResult<()> {
        self.key.validate()?;
        //32KiB Provider input minus2KiB for bounded IDs/context leaves30KiB verbatim user input.
        if self.fingerprint.len() != 64
            || !self.fingerprint.bytes().all(|c| c.is_ascii_hexdigit())
            || self.text.trim().is_empty()
            || self.text.len() > 30 * 1024
            || self
                .text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err(invalid("invalid_feedback"));
        }
        Ok(())
    }
    pub fn message(&self) -> String {
        format!("用户对已保存成果提出修改。请在当前会话继续处理这些意见；这是新的任务回合。\n原任务：{}\n成果指纹：{}\n\n{}", self.key.turn_id, self.fingerprint, self.text)
    }
}

impl MobileCommandStore {
    /// Persist the original opinion before any Provider input; an existing operation never changes it.
    pub fn prepare_delivery_feedback(
        &self,
        source: &NewMobileCommand,
        feedback: &DeliveryFeedback,
    ) -> StorageResult<()> {
        validate(source)?;
        feedback.validate()?;
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (receipt, owner, hash) = read(&tx, &source.device_id, &source.operation_id)?
            .ok_or_else(|| invalid("command_not_reserved"))?;
        if receipt.workspace_id != source.workspace_id
            || receipt.route_id != source.route_id
            || owner != source.owner_id
            || hash != source.request_hash
            || receipt.state != "accepted"
        {
            return Err(invalid("command_changed"));
        }
        let delivery =
            crate::mobile_task_delivery_store::read(&tx, &source.workspace_id, &feedback.key)?
                .ok_or_else(|| invalid("delivery_missing"))?;
        if delivery.outcome.key != feedback.key || delivery.fingerprint != feedback.fingerprint {
            return Err(invalid("delivery_changed"));
        }
        if !matches!(delivery.content, TaskDeliveryContent::Available { .. }) {
            return Err(invalid("delivery_not_ready"));
        }
        if let Some(saved) = read_feedback(&tx, &source.device_id, &source.operation_id)? {
            return if saved == *feedback {
                Ok(())
            } else {
                Err(invalid("feedback_changed"))
            };
        }
        tx.execute("INSERT INTO mobile_delivery_feedback(device_id,operation_id,data_json) VALUES(?1,?2,?3)",
            params![source.device_id,source.operation_id,serde_json::to_string(feedback)?])?;
        tx.commit()?;
        Ok(())
    }
    pub fn delivery_feedback(
        &self,
        device: &str,
        operation: &str,
        workspace: &str,
    ) -> StorageResult<Option<DeliveryFeedback>> {
        for id in [device, operation, workspace] {
            crate::mobile_access_store::validate_id(id)?;
        }
        let connection = open_app_connection(&self.path)?;
        if !read(&connection, device, operation)?
            .is_some_and(|(r, _, _)| r.workspace_id == workspace)
        {
            return Ok(None);
        }
        read_feedback(&connection, device, operation)
    }
}

/// Add the immutable origin only to a confirmed new turn in the original Provider session.
pub(super) fn bind_result(
    connection: &Connection,
    source: &NewMobileCommand,
    result: &mut Option<Value>,
) -> StorageResult<()> {
    let Some(result) = result else {
        return Ok(());
    };
    let Some(feedback) = read_feedback(connection, &source.device_id, &source.operation_id)? else {
        return Ok(());
    };
    let key = &feedback.key;
    let turn = result["turn_id"]
        .as_str()
        .ok_or_else(|| invalid("feedback_result_changed"))?;
    crate::mobile_access_store::validate_id(turn)?;
    if result["node_id"].as_u64() != Some(key.node_id)
        || result["run_id"].as_str() != Some(&key.run_id)
        || result["session_id"].as_str() != Some(&key.session_id)
        || turn == key.turn_id
    {
        return Err(invalid("feedback_result_changed"));
    }
    result["feedback"] = json!({"key":key,"fingerprint":feedback.fingerprint});
    Ok(())
}

fn read_feedback(
    connection: &Connection,
    device: &str,
    operation: &str,
) -> StorageResult<Option<DeliveryFeedback>> {
    let raw: Option<String> = connection
        .query_row(
            "SELECT data_json FROM mobile_delivery_feedback WHERE device_id=?1 AND operation_id=?2",
            params![device, operation],
            |row| row.get(0),
        )
        .optional()?;
    raw.map(|raw| {
        let feedback: DeliveryFeedback = serde_json::from_str(&raw)?;
        feedback.validate()?;
        Ok(feedback)
    })
    .transpose()
}
pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS mobile_delivery_feedback (
        device_id TEXT NOT NULL,operation_id TEXT NOT NULL,data_json TEXT NOT NULL,
        PRIMARY KEY(device_id,operation_id),
        FOREIGN KEY(device_id,operation_id) REFERENCES mobile_commands(device_id,operation_id)
    )",
    )?;
    Ok(())
}
