use norma::connector::{CredentialStore, TokenHash};
use std::{collections::HashMap, net::IpAddr, sync::Mutex};
#[derive(Default)]
pub struct Credentials {
    pub hashes: Mutex<Vec<TokenHash>>,
    pub audit: Mutex<Vec<String>>,
    pub auth_failures: Mutex<HashMap<(IpAddr, i64), u64>>,
}
impl CredentialStore for Credentials {
    fn tokens(&self) -> anyhow::Result<Vec<TokenHash>> {
        Ok(self.hashes.lock().expect("哈希").clone())
    }
    fn replace_tokens(&self, tokens: &[TokenHash]) -> anyhow::Result<()> {
        *self.hashes.lock().expect("哈希") = tokens.to_vec();
        Ok(())
    }
    fn audit_auth_failure(
        &self,
        _: i64,
        source: IpAddr,
        window_start: i64,
        count: u64,
        _: i64,
    ) -> anyhow::Result<()> {
        if self
            .auth_failures
            .lock()
            .expect("失败窗口")
            .insert((source, window_start), count)
            .is_none()
        {
            self.audit.lock().expect("审计").push("unauthorized".into());
        }
        Ok(())
    }
    fn audit(&self, _: i64, _: &str, _: &str, result: &str) -> anyhow::Result<()> {
        self.audit.lock().expect("审计").push(result.into());
        Ok(())
    }
}
