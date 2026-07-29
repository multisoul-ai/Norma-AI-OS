use super::BrowserSurfaceKind;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct BrowserSpec {
    pub url: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub surface: BrowserSurfaceKind,
}

impl BrowserSpec {
    pub(super) fn display_title(&self) -> String {
        if let BrowserSurfaceKind::EgoLite(ego) = &self.surface {
            return ego
                .task_space_name
                .as_deref()
                .filter(|title| !title.trim().is_empty())
                .unwrap_or("Ego")
                .to_string();
        }
        self.title
            .as_deref()
            .filter(|title| !title.trim().is_empty())
            .map(str::to_string)
            .or_else(|| browser_host(&self.url))
            .unwrap_or_else(|| "Browser".to_string())
    }
}

fn browser_host(url: &str) -> Option<String> {
    let (_, rest) = url.split_once("://")?;
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .split('@')
        .next_back()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .trim();
    (!host.is_empty() && !host.chars().any(char::is_whitespace)).then(|| host.to_string())
}
