use crate::protocol::{CardCommandError, PresentationErrorCode, PresentationTarget, PreviewType};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::path::Path;

pub const DEFAULT_BROWSER_URL: &str = "https://www.google.com";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresentationDiagnostics {
    pub scheme: Option<String>,
    pub host: Option<String>,
    pub file_name: Option<String>,
    pub target_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PresentationPreparationError {
    InvalidUrl { message: String },
    ForbiddenUrlScheme { scheme: String },
    PreviewNotFound,
    PreviewNotFile,
    PreviewUnreadable { message: String },
    UnsupportedPreviewType,
}

impl PresentationPreparationError {
    pub fn into_card_error(self) -> CardCommandError {
        let (code, message) = match self {
            Self::InvalidUrl { message } => (PresentationErrorCode::InvalidUrl, message),
            Self::ForbiddenUrlScheme { scheme } => (
                PresentationErrorCode::ForbiddenUrlScheme,
                format!("Browser URL scheme '{scheme}' is not allowed"),
            ),
            Self::PreviewNotFound => (
                PresentationErrorCode::PreviewNotFound,
                "Preview file does not exist".to_string(),
            ),
            Self::PreviewNotFile => (
                PresentationErrorCode::PreviewNotFile,
                "Preview path is not a file".to_string(),
            ),
            Self::PreviewUnreadable { message } => {
                (PresentationErrorCode::PreviewUnreadable, message)
            }
            Self::UnsupportedPreviewType => (
                PresentationErrorCode::UnsupportedPreviewType,
                "Preview supports Markdown, UTF-8 text, and images only".to_string(),
            ),
        };
        CardCommandError::presentation(code, message, false)
    }
}

pub fn prepare_presentation_target(
    target: PresentationTarget,
) -> Result<PresentationTarget, PresentationPreparationError> {
    match target {
        PresentationTarget::Browser { url } => {
            normalize_browser_url(&url).map(|url| PresentationTarget::Browser { url })
        }
        PresentationTarget::Preview {
            path,
            explicit_type,
        } => prepare_preview_path(&path, explicit_type).map(|path| PresentationTarget::Preview {
            path,
            explicit_type,
        }),
    }
}

pub fn normalize_browser_url(input: &str) -> Result<String, PresentationPreparationError> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed == DEFAULT_BROWSER_URL {
        return Ok(DEFAULT_BROWSER_URL.to_string());
    }

    let explicit_scheme = (!looks_like_hostname_with_port(trimmed))
        .then(|| explicit_scheme(trimmed))
        .flatten();
    if let Some(scheme) = explicit_scheme {
        let normalized_scheme = scheme.to_ascii_lowercase();
        if matches!(normalized_scheme.as_str(), "data" | "javascript") {
            return Err(PresentationPreparationError::ForbiddenUrlScheme {
                scheme: normalized_scheme,
            });
        }
    }

    let candidate = if explicit_scheme.is_some() {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    url::Url::parse(&candidate)
        .map(|url| url.to_string())
        .map_err(|error| PresentationPreparationError::InvalidUrl {
            message: format!("Invalid Browser URL: {error}"),
        })
}

pub fn validate_prepared_target(
    target: &PresentationTarget,
) -> Result<(), PresentationPreparationError> {
    match target {
        PresentationTarget::Browser { url } => {
            let normalized = normalize_browser_url(url)?;
            if normalized == *url {
                Ok(())
            } else {
                Err(PresentationPreparationError::InvalidUrl {
                    message: "Browser URL must be normalized before presentation".to_string(),
                })
            }
        }
        PresentationTarget::Preview {
            path,
            explicit_type,
        } => {
            if !path.is_absolute() {
                return Err(PresentationPreparationError::PreviewUnreadable {
                    message: "Preview path must be an absolute canonical path".to_string(),
                });
            }
            validate_preview_type(path, *explicit_type)
        }
    }
}

pub fn presentation_diagnostics(target: &PresentationTarget) -> PresentationDiagnostics {
    let (scheme, host, file_name, raw_target) = match target {
        PresentationTarget::Browser { url } => {
            let parsed = url::Url::parse(url).ok();
            (
                parsed.as_ref().map(|url| url.scheme().to_string()),
                parsed
                    .as_ref()
                    .and_then(|url| url.host_str())
                    .map(str::to_string),
                None,
                url.clone(),
            )
        }
        PresentationTarget::Preview { path, .. } => (
            None,
            None,
            path.file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string),
            path.to_string_lossy().into_owned(),
        ),
    };
    let digest = Sha256::digest(raw_target.as_bytes());
    PresentationDiagnostics {
        scheme,
        host,
        file_name,
        target_sha256: hex::encode(digest),
    }
}

fn prepare_preview_path(
    path: &Path,
    explicit_type: Option<PreviewType>,
) -> Result<std::path::PathBuf, PresentationPreparationError> {
    let canonical = std::fs::canonicalize(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => PresentationPreparationError::PreviewNotFound,
        _ => PresentationPreparationError::PreviewUnreadable {
            message: format!("Preview file is not readable: {error}"),
        },
    })?;
    let metadata = std::fs::metadata(&canonical).map_err(|error| {
        PresentationPreparationError::PreviewUnreadable {
            message: format!("Preview metadata is not readable: {error}"),
        }
    })?;
    if !metadata.is_file() {
        return Err(PresentationPreparationError::PreviewNotFile);
    }
    validate_preview_type(&canonical, explicit_type)?;
    File::open(&canonical).map_err(|error| PresentationPreparationError::PreviewUnreadable {
        message: format!("Preview file is not readable: {error}"),
    })?;
    Ok(canonical)
}

fn validate_preview_type(
    path: &Path,
    explicit_type: Option<PreviewType>,
) -> Result<(), PresentationPreparationError> {
    if explicit_type.is_some() {
        return Ok(());
    }
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or(PresentationPreparationError::UnsupportedPreviewType)?;
    if matches!(
        extension.as_str(),
        "md" | "markdown"
            | "mdown"
            | "mkd"
            | "txt"
            | "text"
            | "log"
            | "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "bmp"
            | "tif"
            | "tiff"
            | "svg"
    ) {
        Ok(())
    } else {
        Err(PresentationPreparationError::UnsupportedPreviewType)
    }
}

fn explicit_scheme(input: &str) -> Option<&str> {
    let colon = input.find(':')?;
    let scheme = &input[..colon];
    let mut chars = scheme.chars();
    if !chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
    {
        return None;
    }
    chars
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.'))
        .then_some(scheme)
}

fn looks_like_hostname_with_port(input: &str) -> bool {
    let Some((host, remainder)) = input.split_once(':') else {
        return false;
    };
    if !(host.eq_ignore_ascii_case("localhost") || host.contains('.')) {
        return false;
    }
    let port = remainder.split(['/', '?', '#']).next().unwrap_or_default();
    !port.is_empty() && port.chars().all(|character| character.is_ascii_digit())
}
