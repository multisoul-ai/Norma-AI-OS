/// Mint an identity independent of node counters, names, definitions and process lifetimes.
pub fn new_card_instance_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
