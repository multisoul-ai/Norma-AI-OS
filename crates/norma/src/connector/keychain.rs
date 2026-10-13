//! Credentials are separate from SQLite and UI state. Implementations must not log values.
use std::{collections::HashMap, sync::Mutex};
#[cfg(target_os = "macos")]
pub const SERVICE: &str = "ai.multisoul.norma.connector";
pub trait Keychain: Send + Sync {
    fn set(&self, account: &str, value: &[u8]) -> anyhow::Result<()>;
    fn get(&self, account: &str) -> anyhow::Result<Option<Vec<u8>>>;
    fn delete(&self, account: &str) -> anyhow::Result<()>;
}
/// Injectable boundary for Linux tests; production macOS always uses MacKeychain.
#[derive(Default)]
pub struct MemoryKeychain(Mutex<HashMap<String, Vec<u8>>>);
impl Keychain for MemoryKeychain {
    fn set(&self, account: &str, value: &[u8]) -> anyhow::Result<()> {
        self.0
            .lock()
            .map_err(|_| anyhow::anyhow!("credential lock"))?
            .insert(account.into(), value.into());
        Ok(())
    }
    fn get(&self, account: &str) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("credential lock"))?
            .get(account)
            .cloned())
    }
    fn delete(&self, account: &str) -> anyhow::Result<()> {
        self.0
            .lock()
            .map_err(|_| anyhow::anyhow!("credential lock"))?
            .remove(account);
        Ok(())
    }
}
#[cfg(target_os = "macos")]
pub struct MacKeychain;
#[cfg(target_os = "macos")]
impl Keychain for MacKeychain {
    fn set(&self, account: &str, value: &[u8]) -> anyhow::Result<()> {
        security_framework::passwords::set_generic_password(SERVICE, account, value)?;
        Ok(())
    }
    fn get(&self, account: &str) -> anyhow::Result<Option<Vec<u8>>> {
        match security_framework::passwords::get_generic_password(SERVICE, account) {
            Ok(value) => Ok(Some(value)),
            Err(error) if error.code() == -25300 => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    fn delete(&self, account: &str) -> anyhow::Result<()> {
        match security_framework::passwords::delete_generic_password(SERVICE, account) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == -25300 => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}
