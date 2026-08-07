use crate::ProviderConfig;
use rig_core::message::{Message, UserContent};

const DEFAULT_CONTEXT_WINDOW_TOKENS: usize = 16_384;
const HISTORY_CONTEXT_FRACTION_NUMERATOR: usize = 3;
const HISTORY_CONTEXT_FRACTION_DENOMINATOR: usize = 4;
const CONSERVATIVE_CHARS_PER_TOKEN: usize = 2;

pub(crate) fn history_char_budget(config: &ProviderConfig) -> usize {
    config
        .context_window_tokens()
        .unwrap_or(DEFAULT_CONTEXT_WINDOW_TOKENS)
        .saturating_mul(HISTORY_CONTEXT_FRACTION_NUMERATOR)
        / HISTORY_CONTEXT_FRACTION_DENOMINATOR
        * CONSERVATIVE_CHARS_PER_TOKEN
}

pub(crate) fn protocol_history_chars(history: &[Message]) -> usize {
    history
        .iter()
        .map(|message| serde_json::to_string(message).map_or(0, |serialized| serialized.len()))
        .sum()
}

pub(crate) fn is_human_user_message(message: &Message) -> bool {
    matches!(
        message,
        Message::User { content }
            if content
                .iter()
                .any(|item| !matches!(item, UserContent::ToolResult(_)))
    )
}
