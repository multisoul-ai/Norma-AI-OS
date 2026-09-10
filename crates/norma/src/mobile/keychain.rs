use super::ServerIdentity;
use anyhow::{ensure, Result};
use core_foundation::data::CFData;
use security_framework::{
    item::{ItemAddOptions, ItemAddValue, ItemClass},
    passwords::get_generic_password,
};
use serde::{Deserialize, Serialize};

const SERVICE: &str = "com.norma-ai.os.mobile-server";
const ITEM_NOT_FOUND: i32 = -25_300;
const DUPLICATE_ITEM: i32 = -25_299;

// Only this private DTO is serializable; ServerIdentity cannot enter a network snapshot.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IdentityRecord {
    certificate: Vec<u8>,
    private_key: Vec<u8>,
}

impl ServerIdentity {
    /// Background only. Caller uses a stable installation account, separate for test data.
    /// Keychain denial/corruption is an error, never a fallback to a disk secret or new identity.
    pub fn load_or_create(account: &str) -> Result<Self> {
        ensure!(
            !account.is_empty()
                && account.len() <= 128
                && account
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
            "invalid mobile Keychain account"
        );
        match get_generic_password(SERVICE, account) {
            Ok(bytes) => return decode(&bytes),
            Err(error) if error.code() == ITEM_NOT_FOUND => {}
            Err(error) => return Err(error.into()),
        }
        let identity = Self::generate()?;
        let record = IdentityRecord {
            certificate: identity.certificate.clone(),
            private_key: identity.private_key.clone(),
        };
        let encoded = serde_json::to_vec(&record)?;
        // SecItemAdd is insert-only: simultaneous starts cannot overwrite each other's key.
        // Generic passwords in the local default Keychain are not iCloud-synchronized.
        let mut item = ItemAddOptions::new(ItemAddValue::Data {
            class: ItemClass::generic_password(),
            data: CFData::from_buffer(&encoded),
        });
        item.set_service(SERVICE).set_account_name(account);
        match item.add() {
            Ok(()) => Ok(identity),
            Err(error) if error.code() == DUPLICATE_ITEM => {
                decode(&get_generic_password(SERVICE, account)?)
            }
            Err(error) => Err(error.into()),
        }
    }
}

fn decode(bytes: &[u8]) -> Result<ServerIdentity> {
    // DER certificates are normally <1 KiB and P-256 keys <200 bytes. The JSON byte arrays
    // expand up to 4x; cap the record at 64 KiB and each decoded field independently.
    ensure!(bytes.len() <= 64 * 1024, "invalid mobile Keychain record");
    let record: IdentityRecord = serde_json::from_slice(bytes)
        .map_err(|_| anyhow::anyhow!("invalid mobile Keychain record"))?;
    ensure!(
        record.certificate.len() <= 16 * 1024 && record.private_key.len() <= 4 * 1024,
        "invalid mobile Keychain record"
    );
    let identity = ServerIdentity {
        certificate: record.certificate,
        private_key: record.private_key,
    };
    // Rustls checks parsing and certificate/private-key consistency before accepting a record.
    identity.server_config()?;
    Ok(identity)
}
