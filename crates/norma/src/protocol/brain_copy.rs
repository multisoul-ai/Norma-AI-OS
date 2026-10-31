//! User-facing decision copy has one character budget; executable payloads stay intact.
use super::BrainOp;
use serde::Serialize;
use std::fmt;

pub const DECISION_COPY_LIMIT: usize = 50;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionCopyIssue {
    Empty,
    Multiline,
    TooLong,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DecisionCopyError {
    pub field: &'static str,
    pub issue: DecisionCopyIssue,
    pub actual: usize,
    pub limit: usize,
}

impl fmt::Display for DecisionCopyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "decision_copy_{:?}: field={} actual={} limit={}",
            self.issue, self.field, self.actual, self.limit
        )
    }
}

impl std::error::Error for DecisionCopyError {}

pub fn validate_decision_copy(field: &'static str, text: &str) -> Result<(), DecisionCopyError> {
    let actual = text.trim().chars().count();
    let issue = if actual == 0 {
        Some(DecisionCopyIssue::Empty)
    } else if text.contains([
        '\n', '\r', '\u{000b}', '\u{000c}', '\u{0085}', '\u{2028}', '\u{2029}',
    ]) {
        Some(DecisionCopyIssue::Multiline)
    } else if actual > DECISION_COPY_LIMIT {
        Some(DecisionCopyIssue::TooLong)
    } else {
        None
    };
    match issue {
        Some(issue) => Err(DecisionCopyError {
            field,
            issue,
            actual,
            limit: DECISION_COPY_LIMIT,
        }),
        None => Ok(()),
    }
}

pub fn validate_proposal_copy(title: &str, why: &str) -> Result<(), DecisionCopyError> {
    validate_decision_copy("title", title)?;
    validate_decision_copy("why", why)?;
    let actual = title.trim().chars().count() + 1 + why.trim().chars().count();
    if actual > DECISION_COPY_LIMIT {
        return Err(DecisionCopyError {
            field: "title+why",
            issue: DecisionCopyIssue::TooLong,
            actual,
            limit: DECISION_COPY_LIMIT,
        });
    }
    Ok(())
}

impl BrainOp {
    pub fn validate_decision_copy(&self) -> Result<(), DecisionCopyError> {
        match self {
            Self::Propose { title, why, .. } => validate_proposal_copy(title, why),
            Self::ProposeAnswer { reason, .. } | Self::Decide { reason, .. } => {
                validate_decision_copy("reason", reason)
            }
            _ => Ok(()),
        }
    }
}
