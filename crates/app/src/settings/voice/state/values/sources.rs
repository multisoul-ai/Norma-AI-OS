use super::EnvVoiceSettings;

impl EnvVoiceSettings {
    pub(crate) fn from_env() -> Self {
        Self {
            ollama_endpoint: std::env::var("MULTISOUL_OLLAMA_ENDPOINT").ok(),
            planner_model: std::env::var("MULTISOUL_OLLAMA_MODEL").ok(),
            dialogue_model: std::env::var("MULTISOUL_DIALOGUE_MODEL").ok(),
            refine_model: std::env::var("MULTISOUL_REFINE_MODEL").ok(),
            input_device_id: std::env::var("MULTISOUL_VOICE_INPUT_DEVICE_ID").ok(),
            trace_enabled: std::env::var("MULTISOUL_VOICE_TRACE").ok(),
        }
    }
}
pub(super) fn non_empty_option(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(super) fn first_non_empty(saved: Option<&str>, env: Option<&str>, default: &str) -> String {
    saved
        .filter(|value| !value.trim().is_empty())
        .or_else(|| env.filter(|value| !value.trim().is_empty()))
        .map(str::trim)
        .unwrap_or(default)
        .to_string()
}

pub(super) fn stored_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

pub(super) fn trace_enabled_from_value(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}
