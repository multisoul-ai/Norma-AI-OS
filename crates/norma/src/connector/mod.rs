//! Opt-in, App-owned read-only MCP connector. No process spawning or terminal input APIs.
mod auth;
mod identity;
mod keychain;
mod server;
mod tools;
pub use auth::{Auth, Authenticated, Clock, SystemClock};
pub use auth::{CredentialStore, TokenHash};
pub use identity::{CardIdentity, CardSigner};
#[cfg(target_os = "macos")]
pub use keychain::MacKeychain;
pub use keychain::{Keychain, MemoryKeychain};
pub use server::ConnectorServer;
pub use tools::{Card, ConnectorTools, DataSource, Project, Snapshot, Task, Workspace};
#[derive(Clone)]
pub struct ConnectorConfig {
    pub enabled: bool,
    pub port: u16,
}
impl Default for ConnectorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 47615,
        }
    }
}
