use crate::{CardToolRequest, CardToolResult};
use thiserror::Error;

pub trait CardToolHost: Send {
    fn execute(&mut self, request: CardToolRequest) -> Result<CardToolResult, CardToolError>;
}

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum CardToolError {
    #[error("invalid tool arguments: {0}")]
    InvalidArguments(String),
    #[error("unknown card tool: {0}")]
    UnknownTool(String),
    #[error("target changed while task was running: {0}")]
    Conflict(String),
    #[error("card tool failed: {0}")]
    Execution(String),
}
