use serde_json::{json, Value};

/// Imported generated text has no memory-version provenance yet. Keep the ledger
/// intact, but carry only original user messages and structural receipts forward.
/// The live model session still retains its own history while revisions are stable.
pub(crate) fn for_model(history: &Value) -> Value {
    Value::Array(
        history
            .as_array()
            .into_iter()
            .flatten()
            .map(|fact| {
                let mut item = json!({
                    "cursor":fact["cursor"], "device":fact["device"],
                    "operation":fact["operation"], "kind":fact["kind"],
                    "sender":fact["sender"], "target":fact["target"],
                    "task_ref":fact["task_ref"], "state":fact["state"],
                });
                if fact["kind"] == "received" && fact["cursor"].as_u64().is_some() {
                    item["text"] = fact["text"].clone();
                    item["text_origin"] = json!("original_user_message");
                } else {
                    item["text_status"] = json!("unavailable_missing_source_versions");
                }
                item
            })
            .collect(),
    )
}
