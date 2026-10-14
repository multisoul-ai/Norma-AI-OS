//! Opt-in, App-owned read-only MCP connector. No process spawning or terminal input APIs.
mod auth;
mod forwarding;
mod identity;
mod server;
mod tools;
pub use auth::{Auth, Authenticated, Clock, SystemClock};
pub use auth::{CredentialStore, TokenRecord};
pub use identity::{CardIdentity, CardSigner};
pub use server::ConnectorServer;
pub use tools::{Card, ConnectorTools, DataSource, Project, Snapshot, Task, TaskEvent, Workspace};
#[derive(Clone)]
pub struct ConnectorConfig {
    pub enabled: bool,
    pub port: u16,
    pub tunnel_authority: Option<workspace::HostAuthority>,
}
impl Default for ConnectorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 47615,
            tunnel_authority: None,
        }
    }
}

/// Local service target syntax: https://tailscale.com/kb/1311/tailscale-funnel.
/// The port is the HTTP backend target, not Funnel's external HTTPS listener.
pub fn funnel_command(port: u16) -> String {
    format!("tailscale funnel --bg {port}")
}
