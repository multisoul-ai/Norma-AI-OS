use super::ServerIdentity;
use anyhow::{ensure, Result};
use rustls::{pki_types::PrivatePkcs8KeyDer, SignatureScheme};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

pub const VOICE_GATEWAY_ORIGIN: &str = "https://soulvoiceai.com/norma/voice";

impl ServerIdentity {
    /// Caller must authenticate the paired device and its current control grant first.
    /// The fixed audience and domain prefix prevent reuse as an arbitrary signing oracle.
    pub fn voice_authorization(&self, device: &str) -> Result<Value> {
        ensure!(
            !device.is_empty()
                && device.len() <= 128
                && device
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'),
            "invalid voice device"
        );
        let issued = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        // 24 hours × 60 minutes × 60 seconds: offline calls work for a day; renewal rechecks pairing.
        let expires = issued + 24 * 60 * 60;
        let payload = serde_json::to_vec(&json!({"v":1,"iss":self.machine_id(),"sub":device,
            "aud":VOICE_GATEWAY_ORIGIN,"iat":issued,"exp":expires}))?;
        let key = rustls::crypto::ring::sign::any_supported_type(
            &PrivatePkcs8KeyDer::from(self.private_key.clone()).into(),
        )?;
        let signer = key
            .choose_scheme(&[SignatureScheme::ECDSA_NISTP256_SHA256])
            .ok_or_else(|| anyhow::anyhow!("unsupported mobile signing identity"))?;
        let mut message = b"Norma voice authorization v1\n".to_vec();
        message.extend_from_slice(&payload);
        let signature = signer.sign(&message)?;
        Ok(
            json!({"origin":VOICE_GATEWAY_ORIGIN,"device_id":device,"machine_id":self.machine_id(),
            "expires_at":expires,"token":format!("nva1.{}.{}",hex::encode(payload),hex::encode(signature))}),
        )
    }
}
