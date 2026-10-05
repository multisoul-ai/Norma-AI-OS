use super::ServerIdentity;
use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;

// Unclaimed QR/code entry expires after two minutes; an immutable claimant waits for local approval.
const PAIRING_TTL: Duration = Duration::from_secs(120);

pub struct PairingQr {
    pairing_code: String,
    machine_id: String,
    machine_name: String,
    certificate_sha256: String,
    endpoints: Vec<String>,
    offer_id: String,
    secret: String,
}

impl PairingQr {
    /// LOCAL DISPLAY ONLY. Never send this code through the bootstrap connection or log it.
    pub fn pairing_code(&self) -> &str {
        &self.pairing_code
    }

    /// Sensitive: display as QR. Never log or send to analytics.
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(&serde_json::json!({
            "version":1, "machine_id":self.machine_id, "machine_name":self.machine_name,
            "certificate_sha256":self.certificate_sha256, "endpoints":self.endpoints,
            "offer_id":self.offer_id, "secret":self.secret, "expires_in_seconds":PAIRING_TTL.as_secs()
        }))?)
    }
}

impl fmt::Debug for PairingQr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PairingQr")
            .field("machine_id", &self.machine_id)
            .finish_non_exhaustive()
    }
}

/// The phone generates and saves its token in Keychain *before* claiming the QR.
/// It can then query authentication after a lost confirmation response without re-registering.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairingRequest {
    offer_id: String,
    secret: String,
    device_id: String,
    device_name: String,
    device_token: String,
}

impl fmt::Debug for PairingRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PairingRequest").finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PairingPending {
    pub request_id: String,
    pub device_id: String,
    pub device_name: String,
    /// Display on both devices, never use this short code as an authentication secret.
    pub verification_code: String,
}

/// One desktop-issued offer and at most one claimant. Call from a serialized background owner.
/// All `now` values must come from the host monotonic clock, never a network message.
pub struct PairingSession {
    offer_id: String,
    secret_hash: Option<[u8; 32]>,
    expires: Instant,
    pending: Option<(PairingPending, String)>,
    code_offer: Option<(String, String)>,
    code_attempts: u8,
}

#[derive(Serialize)]
pub struct PairingCodeReply {
    pub message: String,
    pub sealed: String,
}

impl PairingSession {
    pub fn new(
        identity: &ServerIdentity,
        name: &str,
        endpoints: Vec<String>,
        now: Instant,
    ) -> Result<(Self, PairingQr)> {
        validate_name(name)?;
        ensure!(
            !endpoints.is_empty() && endpoints.len() <= 4,
            "invalid pairing endpoints"
        );
        for address in &endpoints {
            ensure!(address.len() <= 256, "invalid pairing endpoint");
            let endpoint = url::Url::parse(address)
                .map_err(|_| anyhow::anyhow!("invalid pairing endpoint"))?;
            ensure!(
                endpoint.scheme() == "wss"
                    && endpoint.host_str().is_some()
                    && endpoint.username().is_empty()
                    && endpoint.password().is_none()
                    && endpoint.path() == "/mobile"
                    && endpoint.query().is_none()
                    && endpoint.fragment().is_none()
                    && endpoint.port() != Some(0),
                "invalid pairing endpoint"
            );
        }
        let secret = random_secret()?;
        let offer_id = random_secret()?;
        let qr = PairingQr {
            pairing_code: mobile_pairing::new_code()?,
            machine_id: identity.machine_id(),
            machine_name: name.into(),
            certificate_sha256: identity.certificate_sha256(),
            endpoints,
            offer_id,
            secret,
        };
        let session = Self {
            offer_id: qr.offer_id.clone(),
            secret_hash: Some(hash(&qr.secret)),
            expires: now + PAIRING_TTL,
            pending: None,
            code_offer: Some((qr.pairing_code.clone(), qr.to_json()?)),
            code_attempts: 0,
        };
        Ok((session, qr))
    }

    pub fn exchange_code(&mut self, message: &str, now: Instant) -> Result<PairingCodeReply> {
        self.check_live(now)?;
        // Shared by every socket/interface. Reconnecting cannot reset the three-guess budget.
        ensure!(
            self.secret_hash.is_some() && self.code_attempts < 3,
            "pairing code unavailable"
        );
        self.code_attempts += 1;
        let (code, offer) = self
            .code_offer
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("pairing code unavailable"))?;
        ensure!(
            message.len() == mobile_pairing::MESSAGE_BYTES * 2,
            "invalid pairing message"
        );
        let reply = mobile_pairing::respond(code, &hex::decode(message)?, offer.as_bytes())?;
        Ok(PairingCodeReply {
            message: hex::encode(reply.message),
            sealed: hex::encode(reply.sealed),
        })
    }

    pub fn claim(&mut self, request: PairingRequest, now: Instant) -> Result<PairingPending> {
        self.check_live(now)?;
        let Some(expected) = self.secret_hash else {
            bail!("pairing offer unavailable")
        };
        ensure!(
            request.offer_id == self.offer_id
                && valid_secret(&request.secret)
                && equal_hash(&expected, &hash(&request.secret)),
            "pairing offer unavailable"
        );
        ensure!(
            valid_id(&request.device_id) && valid_secret(&request.device_token),
            "invalid pairing device"
        );
        validate_name(&request.device_name)?;
        ensure!(
            !equal_hash(&expected, &hash(&request.device_token)),
            "device credential must be independent"
        );
        let request_id = random_secret()?;
        // Bind the short visual comparison to BOTH the QR and the phone's claimed identity/key.
        let comparison = serde_json::to_vec(&(
            &self.offer_id,
            &request.secret,
            &request.device_id,
            &request.device_token,
        ))?;
        let digest = Sha256::digest(comparison);
        let code = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]) % 1_000_000;
        let pending = PairingPending {
            request_id,
            device_id: request.device_id,
            device_name: request.device_name,
            verification_code: format!("{code:06}"),
        };
        self.secret_hash = None;
        self.code_offer = None;
        self.pending = Some((pending.clone(), request.device_token));
        Ok(pending)
    }

    pub fn pending(&mut self, now: Instant) -> Option<PairingPending> {
        self.check_live(now).ok()?;
        self.pending.as_ref().map(|(pending, _)| pending.clone())
    }

    /// LOCAL DESKTOP ONLY. `persist` must commit the desktop-selected project scopes before
    /// returning success. Never expose this operation as a remotely callable approval method.
    pub fn approve<T>(
        &mut self,
        request_id: &str,
        now: Instant,
        persist: impl FnOnce(&str, &str, &str) -> Result<T>,
    ) -> Result<T> {
        self.check_live(now)?;
        let Some((pending, token)) = &self.pending else {
            bail!("pairing request unavailable")
        };
        ensure!(
            pending.request_id == request_id,
            "pairing request unavailable"
        );
        let result = persist(&pending.device_id, &pending.device_name, token)?;
        self.cancel();
        Ok(result)
    }

    /// Refreshing the QR, desktop rejection, turning off mobile access and window closure
    /// must all cancel the outstanding offer. A cancelled session cannot be revived.
    pub fn cancel(&mut self) {
        self.secret_hash = None;
        self.code_offer = None;
        self.pending = None;
    }

    fn check_live(&mut self, now: Instant) -> Result<()> {
        if self.pending.is_none() && now >= self.expires {
            self.cancel();
            bail!("pairing offer expired");
        }
        Ok(())
    }
}

pub(super) fn random_secret() -> Result<String> {
    let mut bytes = [0u8; 32];
    rustls::crypto::ring::default_provider()
        .secure_random
        .fill(&mut bytes)
        .map_err(|_| anyhow::anyhow!("secure random generation failed"))?;
    Ok(hex::encode(bytes))
}

fn hash(secret: &str) -> [u8; 32] {
    Sha256::digest(secret.as_bytes()).into()
}

fn equal_hash(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.ct_eq(b).into()
}

fn valid_secret(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn validate_name(name: &str) -> Result<()> {
    ensure!(
        !name.trim().is_empty()
            && name.chars().count() <= 80
            && !name.chars().any(char::is_control),
        "invalid device name"
    );
    Ok(())
}
