use super::{
    review::{authorize, verify_route},
    Action, Request, Shared,
};
use serde_json::{json, Value};

pub(super) fn list(shared: &Shared, request: &Request) -> Result<Value, &'static str> {
    let Action::SessionArchive {
        device_id,
        token,
        request: scope,
        before,
    } = &request.action
    else {
        return Err("invalid_request");
    };
    if !scope.valid()
        || scope.delivery_key.is_some()
        || before.is_some_and(|n| n == 0 || n > i64::MAX as u64)
    {
        return Err("invalid_request");
    }
    let revision = authorize(shared, request, scope, device_id, token)?;
    verify_route(shared, request, scope, device_id, token, revision)?;
    let database = shared
        .access()
        .map_err(|_| "unavailable")?
        .store
        .path()
        .to_path_buf();
    let records = storage::MobileTurnStore::for_workspace(database, scope.workspace_id.clone())
        .sessions(*before)
        .map_err(|_| "unavailable")?;
    let next = (records.len() == 20)
        .then(|| records.last().map(|row| row.sequence))
        .flatten();
    let items: Vec<_> = records
        .into_iter()
        .map(|row| {
            json!({
                "sequence":row.sequence,"recorded_at_unix":row.recorded_at_unix,
                "key":row.session.key,"title":row.session.title
            })
        })
        .collect();
    if authorize(shared, request, scope, device_id, token)? != revision {
        return Err("unauthorized");
    }
    verify_route(shared, request, scope, device_id, token, revision)?;
    Ok(
        json!({"status":"session_archive","archive":{"request":scope,"items":items,"next_cursor":next}}),
    )
}
