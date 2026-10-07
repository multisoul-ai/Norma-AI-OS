use super::WorkspaceActivityTarget;
use serde_json::json;
use sha2::{Digest, Sha256};

/// Stable SHA-256 of compact JSON using the activity deduplication identity.
/// Review identity ignores the producing agent/run; approval identity includes its request.
/// Project identity is shared across desktop workspace bindings; area_id is never hashed.
/// Callers supply an inbox kind (review, approval or failed) and its complete target.
pub fn inbox_item_key(kind: &str, project_id: &str, target: &WorkspaceActivityTarget) -> String {
    let identity = match kind {
        "review" => json!([kind, project_id, target.fingerprint]),
        "approval" => json!([
            kind,
            project_id,
            target.node_id,
            target.profile,
            target.run_id,
            target.session_id,
            target.turn_id,
            target.request_id
        ]),
        _ => json!([
            kind,
            project_id,
            target.node_id,
            target.profile,
            target.run_id,
            target.session_id,
            target.turn_id
        ]),
    };
    hex::encode(Sha256::digest(identity.to_string().as_bytes()))
}
