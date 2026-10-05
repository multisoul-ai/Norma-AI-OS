use super::*;

pub(super) fn classify_api_key_display(
    text: Option<&str>,
    revealed: bool,
    expected_secret: Option<&str>,
) -> ApiKeyDisplayState {
    match text {
        None => ApiKeyDisplayState::NotRendered,
        Some(value) if revealed && expected_secret == Some(value) => ApiKeyDisplayState::Revealed,
        Some(_) if revealed => ApiKeyDisplayState::Unsafe,
        Some("Not configured") => ApiKeyDisplayState::NotConfigured,
        Some("*********") => ApiKeyDisplayState::Configured,
        Some(value) if !value.is_empty() && value.chars().all(|character| character == '•') => {
            ApiKeyDisplayState::MaskedEditing
        }
        Some(_) => ApiKeyDisplayState::Unsafe,
    }
}
