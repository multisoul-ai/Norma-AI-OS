# Task 2 Report: App State For Project Agent Definitions

## Scope completed

- Preserved the existing Task 2 state implementation in `crates/app/src/settings/agents.rs` and `crates/app/src/settings/agents/state.rs`.
- Added `MultisoulApp.agent_definitions` to app state initialization.
- Bound workspace-open flow so `enter_workspace_for_path(...)` now receives `&mut AgentDefinitionsState` and calls `bind_project_store(project_store.agent_definition_store())` after opening a workspace.
- Updated workspace-open call sites and tests to pass the new state argument.
- Extended the existing workspace-open app test so it proves project-local agent definitions load into app state after opening a workspace.
- Avoided the earlier unused re-export warnings by only re-exporting `AgentDefinitionsState` from `settings/agents.rs`.

## RED evidence

I cannot honestly provide a fresh RED command transcript for the workspace-binding behavior from before the fix.

What I do know:

- The task brief expected an earlier RED stage around missing `settings::agents`.
- By the time I inherited this worktree, Task 2 state code already existed but was uncommitted.
- The user-provided note says `cargo test -p app agent_settings` compiled but matched `0` tests because the filter was wrong. That is not a valid RED signal for behavior.

Concern:

- There is no trustworthy preserved command output showing the newly added workspace-binding assertion failing before implementation in this inherited worktree, so I am not inventing one.

## GREEN evidence

Focused tests:

- `cargo test -p app test_agent_definitions`
  - Result: `2 passed; 0 failed`
- `cargo test -p app test_agent_voice_context`
  - Result: `1 passed; 0 failed`
- `cargo test -p app test_enter_workspace_records_recent_and_switches_to_canvas`
  - Result: `1 passed; 0 failed`

Required checks:

- `cargo check -p app`
  - Result: success
- `bash scripts/check-max-file-lines.sh`
  - Result: success
- `bash scripts/check-no-allow.sh`
  - Result: success

## Warnings / concerns

- `cargo check -p app` still reports dead-code warnings from `crates/app/src/settings/agents/state.rs`.
- These are mostly Task 3-facing state APIs (`AgentDefinitionField`, validation helpers, draft/status accessors, `voice_context`) that are now present before their UI consumers land.
- I removed the avoidable unused re-export warnings, but I did not add fake usage or `#[allow(...)]` to suppress the remaining warnings.

## Files changed

- `crates/app/src/app/mod.rs`
- `crates/app/src/settings/mod.rs`
- `crates/app/src/settings/agents.rs`
- `crates/app/src/settings/agents/state.rs`
- `crates/app/src/workspace/flow.rs`
- `crates/app/tests/unit/agent_settings_tests.rs`
- `crates/app/tests/unit/app_tests.rs`

## Outcome

Task 2 now leaves agent definition state project-local after workspace open, with test coverage for validation, project-local loading, voice-context filtering, and workspace-open binding.

## Review Fix: Facade Re-exports

The Task 2 review found that `settings::agents` did not re-export the promised
facade types. I fixed the facade to export `AgentDefinitionDraft`,
`AgentDefinitionField`, and `AgentVoiceContext` alongside
`AgentDefinitionsState`, then added a focused test proving the types can be
named from the facade.

Verification:

- `cargo test -p app test_agent_settings_facade_reexports_task2_types`
  - Result: `1 passed; 0 failed`
- `cargo check -p app`
  - Result: success
  - Remaining warnings are Task 3-facing dead-code/unused-import warnings from
    the newly exposed APIs before the UI consumes them.
- `bash scripts/check-max-file-lines.sh`
  - Result: success
- `bash scripts/check-no-allow.sh`
  - Result: success
