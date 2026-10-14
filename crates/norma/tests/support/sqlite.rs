use norma::connector::{CredentialStore, TokenRecord};
use std::net::IpAddr;

pub struct SqliteCredentials(pub storage::connector_store::ConnectorStore);
impl CredentialStore for SqliteCredentials {
    fn tokens(&self) -> anyhow::Result<Vec<TokenRecord>> {
        Ok(self.0.tokens()?)
    }
    fn replace_tokens(&self, tokens: &[TokenRecord]) -> anyhow::Result<()> {
        Ok(self.0.replace_tokens(tokens)?)
    }
    fn secret_or_insert(&self, name: &str, candidate: &[u8], now: i64) -> anyhow::Result<Vec<u8>> {
        Ok(self.0.secret_or_insert(name, candidate, now)?)
    }
    fn audit_auth_failure(
        &self,
        now: i64,
        source: IpAddr,
        window_start: i64,
        count: u64,
        locked_until: i64,
    ) -> anyhow::Result<()> {
        Ok(self.0.audit_auth_failure(
            now,
            &source.to_string(),
            window_start,
            count,
            locked_until,
        )?)
    }
    fn audit(&self, now: i64, tool: &str, parameters: &str, result: &str) -> anyhow::Result<()> {
        Ok(self.0.audit_parameters(now, tool, parameters, result)?)
    }
}
