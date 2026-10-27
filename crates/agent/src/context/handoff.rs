use super::{ContextConstraint, ContextFact, WorkingState};
use serde::Serialize;
use std::{collections::BTreeMap, io::Write};

// Includes the original task, labels and escaped provenance, not just constraint text.
const MAX_HANDOFF_BYTES: usize = 32 * 1024;

#[derive(Serialize)]
struct HandoffState<'a> {
    role: &'a str,
    goal: &'a Option<ContextFact>,
    constraints: &'a BTreeMap<String, ContextConstraint>,
}

/// Carry effective user state at host acceptance, including deterministic routes.
/// Reports, history and memory bodies are deliberately outside this task handoff.
pub fn context_task_handoff(
    text: &str,
    states: &[(&str, &WorkingState)],
) -> Result<String, &'static str> {
    if text.trim().is_empty() {
        return Err("context_handoff_empty_task");
    }
    let mut output = BoundedTask(Vec::new());
    output
        .write_all(text.as_bytes())
        .map_err(|_| "context_handoff_too_large")?;
    for (role, state) in states {
        if state.goal.is_none() && state.constraints.is_empty() {
            continue;
        }
        output
            .write_all("\n\n当前用户目标与约束（含来源；与本任务一并遵守）：\n".as_bytes())
            .map_err(|_| "context_handoff_too_large")?;
        serde_json::to_writer(
            &mut output,
            &HandoffState {
                role,
                goal: &state.goal,
                constraints: &state.constraints,
            },
        )
        .map_err(|_| "context_handoff_too_large")?;
    }
    String::from_utf8(output.0).map_err(|_| "context_handoff_invalid_text")
}

// Abort during serialization so an unbounded number of constraints cannot allocate
// an unbounded intermediate String on the foreground. Never truncate a restriction.
struct BoundedTask(Vec<u8>);
impl Write for BoundedTask {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_HANDOFF_BYTES.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other("task handoff exceeds byte budget"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
