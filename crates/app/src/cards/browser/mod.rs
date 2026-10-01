pub mod card;
pub(crate) mod load_bar;
pub(crate) mod occluded;

pub const DEFAULT_BROWSER_URL: &str = norma::presentation::DEFAULT_BROWSER_URL;

pub fn normalize_browser_url(input: &str) -> String {
    norma::presentation::normalize_browser_url(input).unwrap_or_else(|_| {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            DEFAULT_BROWSER_URL.to_string()
        } else {
            trimmed.to_string()
        }
    })
}
