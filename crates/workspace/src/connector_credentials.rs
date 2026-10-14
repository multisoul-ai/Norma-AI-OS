//! Plaintext connector credentials. Debug output must never expose their values.
use std::fmt;

#[derive(Clone, PartialEq, Eq)]
pub struct TokenRecord {
    pub token: String,
    pub created_at: i64,
    /// None is active; Some is the absolute end of the rotation grace period.
    pub expires_at: Option<i64>,
}
impl fmt::Debug for TokenRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenRecord")
            .field("token", &"[REDACTED]")
            .field("created_at", &self.created_at)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}
