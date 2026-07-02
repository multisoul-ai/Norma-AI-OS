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

## Final Review Fix

- Added a row-level `Create card` action in Settings -> Agents that calls `MultisoulApp::spawn_agent_card_from_definition(definition.id, window, cx)` through a dedicated settings handler.
- Added a focused source assertion covering the visible `Create card` action, the stable `settings-agent-create-card` id prefix, and the handler wiring to `spawn_agent_card_from_definition`.
- Split shared row UI helpers into `crates/app/src/settings/agents/components.rs` so `crates/app/src/settings/agents/view.rs` stays under the 500-line guardrail.

### Tests Run

- `cargo test -p app test_agents_settings_row_exposes_create_card_action_and_wires_spawn_handler` -> passed
- `cargo test -p app agent` -> passed
- `cargo check -p app` -> passed
- `cargo fmt --check` -> passed
- `bash scripts/check-max-file-lines.sh` -> passed
