use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ring::hmac;
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CardIdentity {
    pub workspace_id: String,
    pub area_id: String,
    pub canvas_id: String,
    pub node_id: u64,
    pub definition_id: i64,
    pub created_at: i64,
}
#[derive(Clone)]
pub struct CardSigner {
    key: hmac::Key,
}
impl CardSigner {
    pub fn new(secret: &[u8]) -> Self {
        Self {
            key: hmac::Key::new(hmac::HMAC_SHA256, secret),
        }
    }
    pub fn sign(&self, identity: &CardIdentity) -> String {
        let fields = (
            &identity.workspace_id,
            &identity.area_id,
            &identity.canvas_id,
            identity.node_id,
            identity.definition_id,
            identity.created_at,
        );
        let payload =
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&fields).expect("identity serialization"));
        let tag = hmac::sign(&self.key, payload.as_bytes());
        format!("c1.{payload}.{}", hex::encode(&tag.as_ref()[..8]))
    }
    pub fn verify(&self, value: &str) -> Result<CardIdentity, &'static str> {
        if value.len() > 4096 {
            return Err("invalid_argument");
        }
        let parts: Vec<_> = value.split('.').collect();
        if parts.len() != 3 || parts[0] != "c1" {
            return Err("invalid_argument");
        }
        let tag = hmac::sign(&self.key, parts[1].as_bytes());
        let provided = hex::decode(parts[2]).map_err(|_| "invalid_argument")?;
        if !bool::from(tag.as_ref()[..8].ct_eq(&provided)) {
            return Err("target_changed");
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(parts[1])
            .map_err(|_| "invalid_argument")?;
        let (workspace_id, area_id, canvas_id, node_id, definition_id, created_at) =
            serde_json::from_slice::<(String, String, String, u64, i64, i64)>(&bytes)
                .map_err(|_| "invalid_argument")?;
        Ok(CardIdentity {
            workspace_id,
            area_id,
            canvas_id,
            node_id,
            definition_id,
            created_at,
        })
    }
}
