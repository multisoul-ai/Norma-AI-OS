//! Translate structured copy errors only at the App boundary.
use norma::protocol::{DecisionCopyError, DecisionCopyIssue};

pub(super) fn format_error(error: DecisionCopyError) -> String {
    let key = match error.issue {
        DecisionCopyIssue::Empty => "chrome.brain_copy_empty",
        DecisionCopyIssue::Multiline => "chrome.brain_copy_multiline",
        DecisionCopyIssue::TooLong => "chrome.brain_copy_too_long",
    };
    let message = ui::i18n::format(
        key,
        &[
            ("field", error.field.to_owned()),
            ("actual", error.actual.to_string()),
            ("limit", error.limit.to_string()),
        ],
    );
    format!("{error}: {message}")
}
