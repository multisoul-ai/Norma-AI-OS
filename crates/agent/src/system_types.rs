use crate::ProviderError;
use std::time::Duration;
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct CardTaskBudget {
    pub max_model_rounds: usize,
    pub max_tool_calls: usize,
    pub max_consecutive_tool_failures: usize,
    pub max_wall_time: Duration,
}

impl Default for CardTaskBudget {
    fn default() -> Self {
        Self {
            max_model_rounds: 12,
            max_tool_calls: 20,
            max_consecutive_tool_failures: 3,
            max_wall_time: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),
    #[error("invalid command: {0}")]
    InvalidCommand(String),
}
