use super::MAX_TOOL_CALLS;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NormaSessionError {
    #[error("invalid Norma model configuration: {0}")]
    Configuration(String),
    #[error("Norma input must not be empty")]
    EmptyInput,
    #[error("invalid external delegation identity or context size")]
    InvalidDelegation,
    #[error("Norma turn was cancelled")]
    Cancelled,
    #[error("Norma turn exceeded the 60 second budget")]
    Timeout,
    #[error("Norma exceeded the tool-call budget of {MAX_TOOL_CALLS}")]
    ToolBudgetExceeded,
    #[error("Norma stopped after 3 consecutive tool failures")]
    ToolFailureBudgetExceeded,
    #[error("Norma provider or tool loop failed: {0}")]
    Runtime(String),
    #[error("Norma returned no final response")]
    MissingFinalResponse,
    #[error("Norma tool side effect is unknown; the tool chain stopped without retrying")]
    UnknownSideEffect,
}
