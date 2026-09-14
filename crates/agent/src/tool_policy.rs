use crate::{AdapterError, AdapterResult, NormaToolRuntime, ToolContext};
use serde_json::Value;
use std::sync::{Arc, Mutex};

pub const MAX_TOOL_CALLS_PER_TURN: usize = 20;
const MAX_CONSECUTIVE_FAILURES: usize = 3;

/// One policy per user turn, shared by all tools and continuation responses.
/// Callers serialize execution; transport reconnects must retain the same policy.
#[derive(Clone, Default)]
pub struct ToolCallPolicy {
    budget: Arc<Mutex<ToolCallBudget>>,
}

/// Pure budget shared by the in-memory Rig entry and durable realtime call reservations.
#[derive(Default)]
pub struct ToolCallBudget {
    calls: usize,
    failures: usize,
    unknown: bool,
}
impl ToolCallBudget {
    pub fn reserve(&mut self) -> Result<(), AdapterError> {
        if self.unknown {
            return Err(unknown_error());
        }
        if self.failures >= MAX_CONSECUTIVE_FAILURES {
            return Err(AdapterError::Rejected(
                "Norma 连续工具失败预算已耗尽".into(),
            ));
        }
        if self.calls >= MAX_TOOL_CALLS_PER_TURN {
            return Err(AdapterError::Rejected(
                "Norma 单回合工具调用预算已耗尽".into(),
            ));
        }
        self.calls += 1;
        Ok(())
    }
    pub fn observe(&mut self, status: &str) {
        match status {
            "unknown" => self.unknown = true,
            "rejected" => self.failures += 1,
            _ => self.failures = 0,
        }
    }
}

impl ToolCallPolicy {
    pub fn unknown_seen(&self) -> bool {
        self.budget
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .unknown
    }
    pub fn failure_budget_exhausted(&self) -> bool {
        self.budget
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .failures
            >= MAX_CONSECUTIVE_FAILURES
    }
    pub async fn execute(
        &self,
        runtime: &NormaToolRuntime,
        name: &str,
        args: Value,
        context: &ToolContext,
    ) -> Result<AdapterResult, AdapterError> {
        self.budget
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .reserve()?;
        let result = runtime.execute(name, args, context).await;
        self.budget
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .observe(
                result
                    .as_ref()
                    .map(|value| value.status.as_str())
                    .unwrap_or("rejected"),
            );
        if self.unknown_seen() {
            return Err(unknown_error());
        }
        result
    }
}

fn unknown_error() -> AdapterError {
    AdapterError::Failed("AgentCard Adapter 回执为 unknown，停止后续工具调用".into())
}
