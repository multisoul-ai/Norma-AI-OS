use super::*;
use crate::{MobileTurnKey, MobileTurnState, TaskDeliveryContent};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeliveryAcceptance {
    pub workspace_id: String,
    pub route_id: String,
    pub key: MobileTurnKey,
    pub fingerprint: String,
    pub device_id: String,
    pub operation_id: String,
    pub accepted_at_unix: u64,
}
impl DeliveryAcceptance {
    pub(crate) fn validate(&self) -> StorageResult<()> {
        self.key.validate()?;
        crate::mobile_turn_store::validate_workspace_scope(&self.workspace_id)?;
        for id in [&self.route_id, &self.device_id, &self.operation_id] {
            crate::mobile_access_store::validate_id(id)?;
        }
        if self.accepted_at_unix == 0
            || self.accepted_at_unix > i64::MAX as u64
            || self.fingerprint.len() != 64
            || !self.fingerprint.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err(invalid("invalid_saved_acceptance"));
        }
        Ok(())
    }
    pub fn result(&self) -> Value {
        json!({"node_id":self.key.node_id,"run_id":self.key.run_id,"session_id":self.key.session_id,
            "turn_id":self.key.turn_id,"fingerprint":self.fingerprint,"accepted_at_unix":self.accepted_at_unix})
    }
}

impl MobileCommandStore {
    /// Called only after live scope/authorization checks. Record and command success share one commit.
    pub fn accept_delivery(
        &self,
        source: &NewMobileCommand,
        key: &MobileTurnKey,
        fingerprint: &str,
    ) -> StorageResult<DeliveryAcceptance> {
        validate(source)?;
        key.validate()?;
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (receipt, owner, hash) = read(&tx, &source.device_id, &source.operation_id)?
            .ok_or_else(|| invalid("command_not_reserved"))?;
        if receipt.workspace_id != source.workspace_id
            || receipt.route_id != source.route_id
            || owner != source.owner_id
            || hash != source.request_hash
        {
            return Err(invalid("command_changed"));
        }
        if let Some(accepted) = read_acceptance(&tx, &source.workspace_id, key)? {
            if accepted.device_id == source.device_id
                && accepted.operation_id == source.operation_id
                && accepted.key == *key
                && accepted.fingerprint == fingerprint
                && receipt.state == "succeeded"
                && receipt.result.as_ref() == Some(&accepted.result())
            {
                return Ok(accepted);
            }
            return Err(invalid("delivery_accepted"));
        }
        if receipt.state != "accepted" {
            return Err(invalid("command_changed"));
        }
        let delivery = crate::mobile_task_delivery_store::read(&tx, &source.workspace_id, key)?
            .ok_or_else(|| invalid("delivery_missing"))?;
        if delivery.outcome.key != *key || delivery.fingerprint != fingerprint {
            return Err(invalid("delivery_changed"));
        }
        if delivery.outcome.state != MobileTurnState::Completed
            || !matches!(&delivery.content, TaskDeliveryContent::Available {review} if !review.conflicted)
        {
            return Err(invalid("delivery_not_ready"));
        }
        let accepted = DeliveryAcceptance {
            workspace_id: source.workspace_id.clone(),
            route_id: source.route_id.clone(),
            key: key.clone(),
            fingerprint: fingerprint.into(),
            device_id: source.device_id.clone(),
            operation_id: source.operation_id.clone(),
            accepted_at_unix: current_unix_timestamp()?,
        };
        tx.execute("INSERT INTO mobile_delivery_acceptances(workspace_id,run_id,turn_id,device_id,operation_id,acceptance_json) VALUES(?1,?2,?3,?4,?5,?6)",params![source.workspace_id,key.run_id,key.turn_id,source.device_id,source.operation_id,serde_json::to_string(&accepted)?])?;
        tx.execute("UPDATE mobile_commands SET state='succeeded',result_json=?1,error=NULL WHERE device_id=?2 AND operation_id=?3",params![serde_json::to_string(&accepted.result())?,source.device_id,source.operation_id])?;
        tx.commit()?;
        Ok(accepted)
    }
    pub fn delivery_acceptance(
        &self,
        workspace: &str,
        key: &MobileTurnKey,
    ) -> StorageResult<Option<DeliveryAcceptance>> {
        crate::mobile_turn_store::validate_workspace_scope(workspace)?;
        key.validate()?;
        let connection = open_app_connection(&self.path)?;
        Ok(read_acceptance(&connection, workspace, key)?.filter(|a| a.key == *key))
    }
}

fn read_acceptance(
    connection: &Connection,
    workspace: &str,
    key: &MobileTurnKey,
) -> StorageResult<Option<DeliveryAcceptance>> {
    let raw:Option<String>=connection.query_row("SELECT acceptance_json FROM mobile_delivery_acceptances WHERE workspace_id=?1 AND run_id=?2 AND turn_id=?3",params![workspace,key.run_id,key.turn_id],|row|row.get(0)).optional()?;
    raw.map(|raw| {
        let accepted: DeliveryAcceptance = serde_json::from_str(&raw)?;
        accepted.validate()?;
        if accepted.workspace_id != workspace
            || accepted.key.run_id != key.run_id
            || accepted.key.turn_id != key.turn_id
        {
            return Err(invalid("invalid_saved_acceptance"));
        }
        Ok(accepted)
    })
    .transpose()
}
pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS mobile_delivery_acceptances (
        workspace_id TEXT NOT NULL,run_id TEXT NOT NULL,turn_id TEXT NOT NULL,device_id TEXT NOT NULL,operation_id TEXT NOT NULL,acceptance_json TEXT NOT NULL,
        PRIMARY KEY(workspace_id,run_id,turn_id),UNIQUE(device_id,operation_id),
        FOREIGN KEY(workspace_id,run_id,turn_id) REFERENCES mobile_task_deliveries(workspace_id,run_id,turn_id),
        FOREIGN KEY(device_id,operation_id) REFERENCES mobile_commands(device_id,operation_id)
    )")?;
    Ok(())
}
