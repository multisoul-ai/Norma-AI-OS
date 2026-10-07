use super::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InboxItemKind {
    Approval,
    Failed,
    Review,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InboxDismissal {
    pub project_id: String,
    pub workspace_id: String,
    pub route_id: String,
    pub kind: InboxItemKind,
    pub item_key: String,
    pub device_id: String,
    pub operation_id: String,
    pub dismissed_at_unix: u64,
}

impl InboxDismissal {
    pub fn validate(&self) -> StorageResult<()> {
        crate::mobile_turn_store::validate_workspace_scope(&self.workspace_id)?;
        for id in [
            &self.project_id,
            &self.route_id,
            &self.device_id,
            &self.operation_id,
        ] {
            crate::mobile_access_store::validate_id(id)?;
        }
        validate_item_key(&self.item_key)?;
        if self.dismissed_at_unix == 0 || self.dismissed_at_unix > i64::MAX as u64 {
            return Err(invalid("invalid_saved_dismissal"));
        }
        Ok(())
    }

    pub fn result(&self) -> Value {
        json!({"project_id":self.project_id,"kind":self.kind,"item_key":self.item_key,"dismissed_at_unix":self.dismissed_at_unix})
    }
}

impl MobileCommandStore {
    /// Called after live scope/authorization checks; changes inbox visibility only, never task state.
    /// The first dismissal wins. Another reserved operation for the same item returns that record
    /// and succeeds with its existing result; the original device, operation and time are preserved.
    /// Identity is (project_id, item_key); workspace_id and route_id describe only the first action.
    pub fn dismiss_inbox_item(
        &self,
        source: &NewMobileCommand,
        project_id: &str,
        kind: InboxItemKind,
        item_key: &str,
    ) -> StorageResult<InboxDismissal> {
        validate(source)?;
        crate::mobile_access_store::validate_id(project_id)?;
        validate_item_key(item_key)?;
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
        let saved = read_dismissal(&tx, project_id, item_key)?;
        if receipt.state != "accepted" {
            return match saved {
                Some(saved)
                    if saved.kind == kind
                        && receipt.state == "succeeded"
                        && receipt.result.as_ref() == Some(&saved.result())
                        && receipt.error.is_none() =>
                {
                    Ok(saved)
                }
                _ => Err(invalid("command_changed")),
            };
        }
        let dismissed = if let Some(saved) = saved {
            if saved.kind != kind {
                return Err(invalid("dismissal_kind_changed"));
            }
            saved
        } else {
            let dismissed = InboxDismissal {
                project_id: project_id.into(),
                workspace_id: source.workspace_id.clone(),
                route_id: source.route_id.clone(),
                kind,
                item_key: item_key.into(),
                device_id: source.device_id.clone(),
                operation_id: source.operation_id.clone(),
                dismissed_at_unix: current_unix_timestamp()?,
            };
            dismissed.validate()?;
            tx.execute("INSERT INTO mobile_inbox_dismissals(project_id,item_key,workspace_id,device_id,operation_id,dismissal_json) VALUES(?1,?2,?3,?4,?5,?6)",
                params![dismissed.project_id,dismissed.item_key,dismissed.workspace_id,dismissed.device_id,dismissed.operation_id,serde_json::to_string(&dismissed)?])?;
            dismissed
        };
        tx.execute("UPDATE mobile_commands SET state='succeeded',result_json=?1,error=NULL WHERE device_id=?2 AND operation_id=?3",
            params![serde_json::to_string(&dismissed.result())?,source.device_id,source.operation_id])?;
        tx.commit()?;
        Ok(dismissed)
    }

    /// Shared by every desktop workspace binding of this project on the current Mac.
    pub fn inbox_dismissed_keys(&self, project_id: &str) -> StorageResult<HashSet<String>> {
        crate::mobile_access_store::validate_id(project_id)?;
        let connection = open_app_connection(&self.path)?;
        let mut statement = connection
            .prepare("SELECT item_key FROM mobile_inbox_dismissals WHERE project_id=?1")?;
        let keys = statement.query_map([project_id], |row| row.get::<_, String>(0))?;
        keys.map(|key| {
            let key = key?;
            validate_item_key(&key)?;
            Ok(key)
        })
        .collect()
    }
}

fn validate_item_key(key: &str) -> StorageResult<()> {
    // Canonical lowercase avoids treating uppercase spelling as a second inbox identity.
    if key.len() != 64
        || !key
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(invalid("invalid_inbox_item_key"));
    }
    Ok(())
}

fn read_dismissal(
    connection: &Connection,
    project_id: &str,
    item_key: &str,
) -> StorageResult<Option<InboxDismissal>> {
    let row = connection.query_row("SELECT workspace_id,device_id,operation_id,dismissal_json FROM mobile_inbox_dismissals WHERE project_id=?1 AND item_key=?2",
        params![project_id,item_key], |row| Ok((row.get::<_,String>(0)?, row.get::<_,String>(1)?, row.get::<_,String>(2)?, row.get::<_,String>(3)?))).optional()?;
    row.map(|(workspace, device, operation, raw)| {
        let saved: InboxDismissal = serde_json::from_str(&raw)?;
        saved.validate()?;
        if saved.project_id != project_id
            || saved.workspace_id != workspace
            || saved.item_key != item_key
            || saved.device_id != device
            || saved.operation_id != operation
        {
            return Err(invalid("invalid_saved_dismissal"));
        }
        Ok(saved)
    })
    .transpose()
}

pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS mobile_inbox_dismissals (
        project_id TEXT NOT NULL,item_key TEXT NOT NULL,workspace_id TEXT NOT NULL,device_id TEXT NOT NULL,operation_id TEXT NOT NULL,dismissal_json TEXT NOT NULL,
        PRIMARY KEY(project_id,item_key),UNIQUE(device_id,operation_id),
        FOREIGN KEY(device_id,operation_id) REFERENCES mobile_commands(device_id,operation_id)
    )")?;
    Ok(())
}
