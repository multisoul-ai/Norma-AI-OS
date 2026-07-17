//! Chrome extension auth protocol and browser-neutral Cookie validation.

use serde::{Deserialize, Serialize};

const PROTOCOL_VERSION: u8 = 1;
const COOKIE_EXPORT_TYPE: &str = "cookie_export";
const COOKIE_REQUEST_TYPE: &str = "get_cookies";
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const MAX_COOKIES_PER_EXPORT: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ChromeAuthRequest {
    version: u8,
    #[serde(rename = "type")]
    kind: &'static str,
    request_id: u64,
    node_id: canvas::NodeId,
    url: String,
}

impl ChromeAuthRequest {
    pub(crate) fn new(request_id: u64, node_id: canvas::NodeId, url: &str) -> Result<Self, String> {
        validate_http_url(url)?;
        Ok(Self {
            version: PROTOCOL_VERSION,
            kind: COOKIE_REQUEST_TYPE,
            request_id,
            node_id,
            url: url.to_string(),
        })
    }

    pub(crate) fn request_id(&self) -> u64 {
        self.request_id
    }

    pub(crate) fn node_id(&self) -> canvas::NodeId {
        self.node_id
    }

    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    pub(crate) fn to_json(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(self).map_err(|_| "Chrome auth request could not be encoded".to_string())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SameSitePolicy {
    Unspecified,
    NoRestriction,
    Lax,
    Strict,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct BrowserCookie {
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) domain: String,
    pub(crate) host_only: bool,
    pub(crate) path: String,
    pub(crate) secure: bool,
    pub(crate) http_only: bool,
    pub(crate) same_site: SameSitePolicy,
    pub(crate) expires_unix_seconds: Option<i64>,
}

impl std::fmt::Debug for BrowserCookie {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BrowserCookie")
            .field("name", &self.name)
            .field("value", &"[REDACTED]")
            .field("domain", &self.domain)
            .field("host_only", &self.host_only)
            .field("path", &self.path)
            .field("secure", &self.secure)
            .field("http_only", &self.http_only)
            .field("same_site", &self.same_site)
            .field("expires_unix_seconds", &self.expires_unix_seconds)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CookieImportBatch {
    pub(crate) cookies: Vec<BrowserCookie>,
    pub(crate) skipped: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum BrowserAuthUiState {
    #[default]
    Idle,
    Waiting,
    Ready,
    Failed,
}

#[derive(Deserialize)]
struct CookieExport {
    version: u8,
    #[serde(rename = "type")]
    kind: String,
    request_id: u64,
    node_id: canvas::NodeId,
    url: String,
    cookies: Vec<ChromeCookie>,
}

#[derive(Deserialize)]
struct ResponseEnvelope {
    request_id: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChromeCookie {
    name: String,
    value: String,
    domain: String,
    host_only: bool,
    path: String,
    secure: bool,
    http_only: bool,
    same_site: ChromeSameSite,
    session: bool,
    expiration_date: Option<f64>,
    store_id: String,
    partition_key: Option<serde_json::Value>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ChromeSameSite {
    Unspecified,
    NoRestriction,
    Lax,
    Strict,
}

pub(crate) fn parse_cookie_export(
    bytes: &[u8],
    request: &ChromeAuthRequest,
    now_unix_seconds: i64,
) -> Result<CookieImportBatch, String> {
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err("Chrome auth response exceeded the 1 MiB safety limit".to_string());
    }
    let export: CookieExport = serde_json::from_slice(bytes)
        .map_err(|_| "Chrome auth response was not valid protocol JSON".to_string())?;
    validate_export_identity(&export, request)?;
    if export.cookies.len() > MAX_COOKIES_PER_EXPORT {
        return Err("Chrome auth response exceeded the cookie count safety limit".to_string());
    }
    let request_url = url::Url::parse(&request.url)
        .map_err(|_| "Pending Chrome auth request contained an invalid URL".to_string())?;

    let mut cookies = Vec::with_capacity(export.cookies.len());
    let mut skipped = 0;
    for cookie in export.cookies {
        match convert_cookie(cookie, &request_url, now_unix_seconds) {
            Some(cookie) => cookies.push(cookie),
            None => skipped += 1,
        }
    }
    Ok(CookieImportBatch { cookies, skipped })
}

pub(crate) fn response_request_id(bytes: &[u8]) -> Result<u64, String> {
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err("Chrome auth response exceeded the 1 MiB safety limit".to_string());
    }
    serde_json::from_slice::<ResponseEnvelope>(bytes)
        .map(|envelope| envelope.request_id)
        .map_err(|_| "Chrome auth response was not valid protocol JSON".to_string())
}

fn validate_http_url(value: &str) -> Result<(), String> {
    let parsed = url::Url::parse(value).map_err(|_| "Browser URL is invalid".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("Chrome auth only supports HTTP(S) Browser URLs".to_string());
    }
    Ok(())
}

fn validate_export_identity(
    export: &CookieExport,
    request: &ChromeAuthRequest,
) -> Result<(), String> {
    if export.version != PROTOCOL_VERSION || export.kind != COOKIE_EXPORT_TYPE {
        return Err("Chrome auth response used an unsupported protocol".to_string());
    }
    if export.request_id != request.request_id
        || export.node_id != request.node_id
        || export.url != request.url
    {
        return Err("Chrome auth response did not match the pending Browser request".to_string());
    }
    Ok(())
}

fn convert_cookie(
    cookie: ChromeCookie,
    request_url: &url::Url,
    now_unix_seconds: i64,
) -> Option<BrowserCookie> {
    let _ = &cookie.store_id;
    if cookie.partition_key.is_some() || cookie.name.is_empty() || cookie.domain.is_empty() {
        return None;
    }
    let request_host = request_url.host_str()?;
    if !cookie_domain_matches(request_host, &cookie.domain, cookie.host_only)
        || !cookie_path_matches(request_url.path(), &cookie.path)
        || (cookie.secure && request_url.scheme() != "https")
    {
        return None;
    }
    let expires_unix_seconds = if cookie.session {
        None
    } else {
        let expires = cookie.expiration_date?;
        if !expires.is_finite() || expires <= now_unix_seconds as f64 || expires > i64::MAX as f64 {
            return None;
        }
        Some(expires as i64)
    };
    Some(BrowserCookie {
        name: cookie.name,
        value: cookie.value,
        domain: cookie.domain,
        host_only: cookie.host_only,
        path: cookie.path,
        secure: cookie.secure,
        http_only: cookie.http_only,
        same_site: match cookie.same_site {
            ChromeSameSite::Unspecified => SameSitePolicy::Unspecified,
            ChromeSameSite::NoRestriction => SameSitePolicy::NoRestriction,
            ChromeSameSite::Lax => SameSitePolicy::Lax,
            ChromeSameSite::Strict => SameSitePolicy::Strict,
        },
        expires_unix_seconds,
    })
}

fn cookie_domain_matches(request_host: &str, cookie_domain: &str, host_only: bool) -> bool {
    let request_host = request_host.to_ascii_lowercase();
    let cookie_domain = cookie_domain
        .strip_prefix('.')
        .unwrap_or(cookie_domain)
        .to_ascii_lowercase();
    if host_only {
        return request_host == cookie_domain;
    }
    request_host == cookie_domain
        || request_host
            .strip_suffix(&cookie_domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

fn cookie_path_matches(request_path: &str, cookie_path: &str) -> bool {
    if !cookie_path.starts_with('/') || !request_path.starts_with(cookie_path) {
        return false;
    }
    request_path.len() == cookie_path.len()
        || cookie_path.ends_with('/')
        || request_path.as_bytes().get(cookie_path.len()) == Some(&b'/')
}
