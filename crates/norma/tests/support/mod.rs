use norma::connector::{CredentialStore, TokenRecord};
use std::{collections::HashMap, net::IpAddr, sync::Mutex};
#[derive(Default)]
pub struct Credentials {
    pub tokens: Mutex<Vec<TokenRecord>>,
    pub secrets: Mutex<HashMap<String, Vec<u8>>>,
    pub audit: Mutex<Vec<String>>,
    pub auth_failures: Mutex<HashMap<(IpAddr, i64), u64>>,
}
impl CredentialStore for Credentials {
    fn tokens(&self) -> anyhow::Result<Vec<TokenRecord>> {
        Ok(self.tokens.lock().expect("哈希").clone())
    }
    fn replace_tokens(&self, tokens: &[TokenRecord]) -> anyhow::Result<()> {
        *self.tokens.lock().expect("哈希") = tokens.to_vec();
        Ok(())
    }
    fn secret_or_insert(&self, name: &str, candidate: &[u8], _: i64) -> anyhow::Result<Vec<u8>> {
        Ok(self
            .secrets
            .lock()
            .expect("密钥")
            .entry(name.into())
            .or_insert_with(|| candidate.to_vec())
            .clone())
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
