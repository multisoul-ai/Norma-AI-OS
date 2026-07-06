//! Browser engine selection and feature-gated backend names.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BrowserEngineKind {
    Wry,
    #[cfg(all(target_os = "macos", feature = "chromium-browser"))]
    Cef,
}

impl BrowserEngineKind {
    pub(crate) fn from_env_value(value: Option<&str>) -> Self {
        match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("cef" | "chromium" | "chrome") => Self::explicit_cef_or_fallback(),
            Some("wry" | "webkit") => Self::Wry,
            _ => Self::default_for_build(),
        }
    }

    fn default_for_build() -> Self {
        Self::Wry
    }

    fn explicit_cef_or_fallback() -> Self {
        #[cfg(all(target_os = "macos", feature = "chromium-browser"))]
        {
            Self::Cef
        }
        #[cfg(not(all(target_os = "macos", feature = "chromium-browser")))]
        {
            Self::Wry
        }
    }
}
