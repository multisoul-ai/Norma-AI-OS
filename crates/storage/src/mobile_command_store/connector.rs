//! Connector namespace over the same reserve/finish state machine. Never enters mobile grants.
use super::*;

fn connector_request(request: &NewMobileCommand) -> StorageResult<()> {
    let epoch = request
        .device_id
        .strip_prefix("connector:")
        .ok_or_else(|| invalid("connector namespace required"))?;
    crate::mobile_access_store::validate_id(epoch)?;
    if request.owner_id != "grokbot"
        || request.operation_id.is_empty()
        || request.operation_id.len() > 128
        || request.operation_id.chars().any(char::is_control)
        || request.route_id.is_empty()
        || request.route_id.len() > 4096
    {
        return Err(invalid("invalid connector command"));
    }
    crate::mobile_turn_store::validate_workspace_scope(&request.workspace_id)?;
    if request.request_hash.len() != 64
        || !request.request_hash.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err(invalid("invalid connector fingerprint"));
    }
    Ok(())
}
impl MobileCommandStore {
    pub fn reserve_connector(
        &self,
        request: &NewMobileCommand,
        now: i64,
    ) -> StorageResult<MobileCommandReservation> {
        connector_request(request)?;
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result = if let Some(existing) = connector_lookup(&tx, request)? {
            existing
        } else {
            tx.execute("INSERT INTO mobile_commands(device_id,operation_id,workspace_id,route_id,owner_id,request_hash,state,created_at_unix) VALUES(?1,?2,?3,?4,?5,?6,'accepted',?7)", params![request.device_id, request.operation_id, request.workspace_id, request.route_id, request.owner_id, request.request_hash, now])?;
            MobileCommandReservation::New
        };
        tx.commit()?;
        Ok(result)
    }
    pub fn lookup_connector(
        &self,
        request: &NewMobileCommand,
    ) -> StorageResult<Option<MobileCommandReservation>> {
        connector_request(request)?;
        connector_lookup(&open_app_connection(&self.path)?, request)
    }
    pub fn finish_connector(
        &self,
        request: &NewMobileCommand,
        outcome: Result<Value, String>,
    ) -> StorageResult<()> {
        connector_request(request)?;
        self.finish_on_validated(request, outcome)
    }
    pub fn connector_receipts(&self, device: &str) -> StorageResult<Vec<MobileCommandReceipt>> {
        let epoch = device
            .strip_prefix("connector:")
            .ok_or_else(|| invalid("connector namespace required"))?;
        crate::mobile_access_store::validate_id(epoch)?;
        let connection = open_app_connection(&self.path)?;
        let operations = connection.prepare("SELECT operation_id FROM mobile_commands WHERE device_id=?1 ORDER BY created_at_unix,operation_id")?.query_map([device], |r| r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;
        operations
            .into_iter()
            .map(|operation| {
                let (mut receipt, _, _) = read(&connection, device, &operation)?
                    .ok_or_else(|| invalid("missing receipt"))?;
                unknown_if_pending(&mut receipt);
                Ok(receipt)
            })
            .collect()
    }
    pub fn connector_operation(
        &self,
        device: &str,
        operation: &str,
        hash: &str,
    ) -> StorageResult<Option<MobileCommandReservation>> {
        let epoch = device
            .strip_prefix("connector:")
            .ok_or_else(|| invalid("connector namespace required"))?;
        crate::mobile_access_store::validate_id(epoch)?;
        let connection = open_app_connection(&self.path)?;
        let Some((mut receipt, _, stored_hash)) = read(&connection, device, operation)? else {
            return Ok(None);
        };
        if hash != stored_hash {
            return Ok(Some(MobileCommandReservation::Conflict));
        }
        unknown_if_pending(&mut receipt);
        Ok(Some(MobileCommandReservation::Existing(Box::new(receipt))))
    }
    pub fn connector_origins(&self) -> StorageResult<Vec<MobileCommandReceipt>> {
        let connection = open_app_connection(&self.path)?;
        let keys = connection.prepare("SELECT device_id,operation_id FROM mobile_commands WHERE device_id GLOB 'connector:*' AND state='succeeded' ORDER BY created_at_unix,operation_id")?.query_map([], |r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<Result<Vec<_>,_>>()?;
        keys.into_iter()
            .map(|(device, operation)| {
                read(&connection, &device, &operation)?
                    .map(|(r, _, _)| r)
                    .ok_or_else(|| invalid("missing origin"))
            })
            .collect()
    }
    pub fn cleanup_connector(&self, now: i64) -> StorageResult<usize> {
        Ok(open_app_connection(&self.path)?.execute("DELETE FROM mobile_commands WHERE device_id GLOB 'connector:*' AND created_at_unix < ?1", [now.saturating_sub(30 * 86400)])?)
    }
}
fn unknown_if_pending(receipt: &mut MobileCommandReceipt) {
    if receipt.state == "accepted" {
        receipt.state = "outcome_unknown".into();
        receipt.error = Some("outcome_unknown".into());
    }
}
fn connector_lookup(
    connection: &Connection,
    request: &NewMobileCommand,
) -> StorageResult<Option<MobileCommandReservation>> {
    let mut result = lookup_on(connection, request)?;
    if let Some(MobileCommandReservation::Existing(receipt)) = &mut result {
        unknown_if_pending(receipt);
    }
    Ok(result)
}
