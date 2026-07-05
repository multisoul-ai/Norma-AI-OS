pub mod card;

pub const DEFAULT_BROWSER_URL: &str = "https://www.google.com";

pub fn normalize_browser_url(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return DEFAULT_BROWSER_URL.to_string();
    }
    if has_web_scheme(trimmed) {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

fn has_web_scheme(input: &str) -> bool {
    input.starts_with("https://") || input.starts_with("http://")
}
