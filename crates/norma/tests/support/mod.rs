use norma::connector::{CredentialStore, TokenHash};
use std::sync::Mutex;
#[derive(Default)]
pub struct Credentials {
    pub hashes: Mutex<Vec<TokenHash>>,
    pub audit: Mutex<Vec<String>>,
}
impl CredentialStore for Credentials {
    fn tokens(&self) -> anyhow::Result<Vec<TokenHash>> {
        Ok(self.hashes.lock().expect("哈希").clone())
    }
    fn replace_tokens(&self, tokens: &[TokenHash]) -> anyhow::Result<()> {
        *self.hashes.lock().expect("哈希") = tokens.to_vec();
        Ok(())
    }
    fn audit(&self, _: i64, _: &str, _: &str, result: &str) -> anyhow::Result<()> {
        self.audit.lock().expect("审计").push(result.into());
        Ok(())
    }
}
