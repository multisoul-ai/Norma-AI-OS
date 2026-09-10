use super::{Action, Request, Shared};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use storage::{MobilePushRegistration, MobilePushStore};

// Missing registration is malformed; explicit null is the user's disable operation.
pub(super) fn registration<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<MobilePushRegistration>, D::Error> {
    Option::<MobilePushRegistration>::deserialize(deserializer)
}

pub(super) fn handle(shared: &Arc<Shared>, request: &Request) -> Result<Value, &'static str> {
    let (device, token, update) = match &request.action {
        Action::PushSubscription { device_id, token } => (device_id, token, None),
        Action::SetPushSubscription {
            device_id,
            token,
            expected_revision,
            registration,
        } => (
            device_id,
            token,
            Some((*expected_revision, registration.as_ref())),
        ),
        _ => unreachable!("only push subscription actions reach this handler"),
    };
    // Serialized with desktop revoke; the storage transaction also rechecks the grant revision.
    let access = shared.access().map_err(|_| "unavailable")?;
    let grant = access
        .authenticate(&request.machine_id, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    let store = MobilePushStore::new(access.store.path().to_path_buf());
    let state = if let Some((revision, registration)) = update {
        if revision >= i64::MAX as u64
            || registration.is_some_and(|registration| registration.validate().is_err())
        {
            return Err("invalid_push_registration");
        }
        store
            .replace(&grant, revision, registration)
            .map_err(|_| "unavailable")?
            .ok_or("push_subscription_changed")?
    } else {
        store
            .get(&grant)
            .map_err(|_| "unavailable")?
            .ok_or("unauthorized")?
    };
    Ok(json!({"status":"push_subscription", "subscription":{
        "machine_id": access.machine_id(), "device_id": grant.device_id,
        "revision": state.revision, "registration": state.registration,
        "registered_at_ms": state.registered_at_ms,
    }}))
}
