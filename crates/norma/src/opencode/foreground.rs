//! Read only the private route published by the original TUI plugin, on a background thread.
use serde::Deserialize;
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Deserialize)]
pub(super) struct Route {
    nonce: String,
    pub revision: u64,
    pub session: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: u64,
}
impl Route {
    pub fn same_position(&self, other: &Self) -> bool {
        self.revision == other.revision && self.session == other.session
    }
}
pub(super) struct Foreground {
    path: PathBuf,
    nonce: String,
    pub current: Option<Route>,
    revision: u64,
}
impl Foreground {
    pub fn new(path: PathBuf, nonce: String) -> Self {
        Self {
            path,
            nonce,
            current: None,
            revision: 0,
        }
    }
    pub fn read(&mut self) -> Option<Route> {
        let file = std::fs::File::open(&self.path).ok()?;
        let mut bytes = Vec::new();
        file.take(8193).read_to_end(&mut bytes).ok()?;
        if bytes.len() > 8192 {
            return None;
        }
        let route: Route = serde_json::from_slice(&bytes).ok()?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_millis();
        if route.nonce != self.nonce
            || route.revision == 0
            || route.revision < self.revision
            || u128::from(route.updated_at) > now + 250
            || now.saturating_sub(u128::from(route.updated_at)) > 1500
            || route.session.as_ref().is_some_and(|s| {
                !s.starts_with("ses_")
                    || s.len() > 4096
                    || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
        {
            return None;
        }
        if self
            .current
            .as_ref()
            .is_some_and(|old| old.revision == route.revision && old.session != route.session)
        {
            return None;
        }
        self.revision = route.revision;
        Some(route)
    }
    pub fn still_current(&mut self) -> bool {
        self.read()
            .zip(self.current.as_ref())
            .is_some_and(|(now, old)| now.same_position(old) && now.session.is_some())
    }
}

/// Install in the owned launch directory; the caller preserves any pre-existing TUI config.
pub fn install(directory: &Path, nonce: &str) -> std::io::Result<PathBuf> {
    let path = directory.join("current-session.json");
    let plugin = directory.join("current-session.mjs");
    let source = include_str!("current_session.mjs")
        .replace(
            "__NORMA_ROUTE_PATH__",
            &serde_json::to_string(&path).map_err(std::io::Error::other)?,
        )
        .replace(
            "__NORMA_ROUTE_NONCE__",
            &serde_json::to_string(nonce).map_err(std::io::Error::other)?,
        );
    std::fs::write(&plugin, source)?;
    let uri = url::Url::from_file_path(plugin)
        .map_err(|_| std::io::Error::other("invalid TUI plugin path"))?;
    std::fs::write(
        directory.join("tui.json"),
        serde_json::json!({"plugin":[uri.as_str()]}).to_string(),
    )?;
    Ok(path)
}
