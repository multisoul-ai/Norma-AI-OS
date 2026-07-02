# Task 6 Report: Verification Guardrail Test Split

## Scope

Resolved the Task 6 verification guardrail failure in `/Users/alan/Documents/mybook/multisoul-desktop/.worktrees/project-agent-catalog` by splitting oversized unit tests out of `crates/app/tests/unit/agent_settings_actions_tests.rs` without changing production behavior.

## Files Changed

- `crates/app/tests/unit/agent_settings_actions_tests.rs`
- `crates/app/tests/unit/agent_settings_pending_edit_tests.rs`
- `crates/app/src/settings/agents.rs`

## Notes

- Moved the pending-edit, delete, and toggle persistence tests into a focused new test module.
- Preserved all test names, comments, assertion messages, and state coverage.
- Kept the change mechanical so `scripts/check-max-file-lines.sh` passes after formatting.
