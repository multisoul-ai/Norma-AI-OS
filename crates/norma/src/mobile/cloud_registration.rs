use super::{ServerIdentity, VOICE_GATEWAY_ORIGIN};
use anyhow::{ensure, Result};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

impl ServerIdentity {
    /// Only called after fresh paired-device authorization. The phone retains its own cloud
    /// secret; the Mac attests its hash for initial registration, not for subsequent model use.
    pub fn cloud_registration(
        &self,
        device: &str,
        client: &str,
        token_hash: &str,
    ) -> Result<String> {
        let hex = |b: u8| b.is_ascii_digit() || (b'a'..=b'f').contains(&b);
        ensure!(
            !device.is_empty()
                && device.len() <= 128
                && device
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
                && client.len() == 36
                && client.bytes().enumerate().all(|(i, b)| {
                    if [8, 13, 18, 23].contains(&i) {
                        b == b'-'
                    } else {
                        hex(b)
                    }
                })
                && token_hash.len() == 64
                && token_hash.bytes().all(hex),
            "invalid cloud registration"
        );
        let issued = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        // 2 minutes × 60 seconds: registration proof expires; the cloud device does not.
        let payload = serde_json::to_vec(&json!({"v":1,"iss":self.machine_id(),"sub":device,
            "aud":VOICE_GATEWAY_ORIGIN,"iat":issued,"exp":issued + 120,
            "client":client,"token_hash":token_hash}))?;
        let mut message = b"Norma cloud registration v1\n".to_vec();
        message.extend_from_slice(&payload);
        Ok(format!(
            "ncr1.{}.{}",
            hex::encode(&payload),
            hex::encode(self.sign(&message)?)
        ))
    }
}
