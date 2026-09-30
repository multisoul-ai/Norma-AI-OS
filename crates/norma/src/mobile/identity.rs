use anyhow::Result;
use rustls::{pki_types::PrivatePkcs8KeyDer, ServerConfig, SignatureScheme};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fmt, sync::Arc};

/// Stable installation identity. Loading, generation and SQLite access happen off the GPUI foreground.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerIdentity {
    pub(super) certificate: Vec<u8>,
    pub(super) private_key: Vec<u8>,
}

impl fmt::Debug for ServerIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerIdentity")
            .field("machine_id", &self.machine_id())
            .finish_non_exhaustive()
    }
}

impl ServerIdentity {
    pub fn from_storage_json(value: &str) -> Result<Self> {
        anyhow::ensure!(value.len() <= 64 * 1024, "invalid mobile identity record");
        let identity: Self = serde_json::from_str(value)
            .map_err(|_| anyhow::anyhow!("invalid mobile identity record"))?;
        anyhow::ensure!(
            identity.certificate.len() <= 16 * 1024 && identity.private_key.len() <= 4 * 1024,
            "invalid mobile identity record"
        );
        identity.server_config()?;
        Ok(identity)
    }

    pub fn storage_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn generate() -> Result<Self> {
        let rcgen::CertifiedKey { cert, key_pair } =
            rcgen::generate_simple_self_signed(vec!["norma.local".into()])?;
        Ok(Self {
            certificate: cert.der().to_vec(),
            private_key: key_pair.serialize_der(),
        })
    }

    pub fn certificate_der(&self) -> &[u8] {
        &self.certificate
    }

    pub fn certificate_sha256(&self) -> String {
        hex::encode(Sha256::digest(&self.certificate))
    }

    /// Never derive this from the IP, machine name, project path or window ID.
    pub fn machine_id(&self) -> String {
        format!("mac-{}", self.certificate_sha256())
    }

    pub(super) fn sign(&self, message: &[u8]) -> Result<Vec<u8>> {
        let key = rustls::crypto::ring::sign::any_supported_type(
            &PrivatePkcs8KeyDer::from(self.private_key.clone()).into(),
        )?;
        let signer = key
            .choose_scheme(&[SignatureScheme::ECDSA_NISTP256_SHA256])
            .ok_or_else(|| anyhow::anyhow!("unsupported mobile signing identity"))?;
        Ok(signer.sign(message)?)
    }

    pub fn server_config(&self) -> Result<Arc<ServerConfig>> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut config = ServerConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS13])?
            .with_no_client_auth()
            .with_single_cert(
                vec![self.certificate.clone().into()],
                PrivatePkcs8KeyDer::from(self.private_key.clone()).into(),
            )?;
        // WebSocket upgrade uses HTTP/1.1. Device authentication follows TLS pinning.
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        config.max_early_data_size = 0;
        Ok(Arc::new(config))
    }
}
