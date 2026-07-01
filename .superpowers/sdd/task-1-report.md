# Task 1 Report: Storage Schema And Store for Project Agent Catalog

## Scope

Implemented exactly Task 1 storage work in `/Users/alan/Documents/mybook/multisoul-desktop/.worktrees/project-agent-catalog`:

- Added `agent_definitions` project-db schema and case-insensitive unique index.
- Added `AgentDefinitionStore` plus `AgentDefinition`, `NewAgentDefinition`, `AgentDefinitionUpdate`.
- Exported the new store/types from `storage::lib`.
- Added `ProjectStore::agent_definition_store()`.
- Added focused storage tests for migration, CRUD round-trip, duplicate-name rejection, and invalid runtime-profile decoding.

Did not implement app settings UI, card creation, or any non-storage behavior.

## TDD Evidence

### RED 1: migration test before schema

Command:

```bash
cargo test -p storage test_agent_definitions_migration_creates_table_and_unique_index
```

Result:

- Exit code: `101`
- Failure observed in `test_agent_definitions_migration_creates_table_and_unique_index`
- Relevant failure text:

```text
agent_definitions 必须包含 name 列作为 Settings 和语音匹配的显示名
```

Interpretation:

- `PRAGMA table_info(agent_definitions)` returned no useful schema rows because the table did not exist yet.
- This is the expected RED for Step 2 of the brief.

### GREEN 1: migration test after schema

Command:

```bash
cargo test -p storage test_agent_definitions_migration_creates_table_and_unique_index
```

Result:

- Exit code: `0`
- `1 passed; 0 failed`

### RED 2: CRUD tests before store implementation

Command:

```bash
cargo test -p storage agent_definition_store
```

Result:

- Exit code: `101`
- Compile-time RED matched the missing implementation state
- Relevant errors included:

```text
cannot find struct, variant or union type `NewAgentDefinition` in this scope
cannot find struct, variant or union type `AgentDefinitionUpdate` in this scope
use of undeclared type `AgentDefinitionStore`
```

Interpretation:

- The new CRUD tests were compiled and failed specifically because Task 1 store types and API were not implemented yet.
- This is the expected RED for Step 6 of the brief.

### GREEN 2: focused AgentDefinitionStore tests after implementation

Commands:

```bash
cargo test -p storage test_agent_definitions_migration_creates_table_and_unique_index
cargo test -p storage agent_definition_store
```

Results:

- Migration test: exit code `0`
- AgentDefinitionStore-focused run: exit code `0`
- `4 passed; 0 failed`

Passing tests:

- `test_agent_definitions_migration_creates_table_and_unique_index`
- `test_agent_definition_store_create_list_update_delete_round_trips`
- `test_agent_definition_store_rejects_case_insensitive_duplicate_names`
- `test_agent_definition_store_invalid_runtime_profile_returns_typed_error`

## Final Verification

Commands run fresh before completion:

```bash
cargo test -p storage
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
```

Results:

- `cargo test -p storage`: exit code `0`, `25 passed; 0 failed`
- `bash scripts/check-max-file-lines.sh`: exit code `0`
- `bash scripts/check-no-allow.sh`: exit code `0`

## Files Changed

- `crates/storage/src/agent_definition_store.rs` (new)
- `crates/storage/tests/unit/agent_definition_store_tests.rs` (new)
- `crates/storage/src/migrations.rs`
- `crates/storage/src/project_store.rs`
- `crates/storage/src/lib.rs`

## Implementation Notes

- Schema added to `migrate_project_db`:
  - table: `agent_definitions`
  - unique index: `idx_agent_definitions_name_nocase`
- `AgentDefinitionStore::create` writes defaults:
  - `enabled = true`
  - `sort_order = 0`
- `list()` orders by:
  - `sort_order ASC`
  - `name COLLATE NOCASE ASC`
- Runtime profile encoding/decoding matches the existing storage convention:
  - `claude`
  - `codex`
  - `opencode`
- Invalid stored runtime profiles surface as `StorageError::InvalidAgentProfile(...)`.

## Self-Review Notes

- Stayed within the task boundary; no UI/runtime/app feature work was added.
- Preserved storage dependency boundaries; no app/runtime/ui imports were introduced.
- Kept Rust files below the 500-line limit:
  - `agent_definition_store.rs`: 192 lines
  - `agent_definition_store_tests.rs`: 267 lines
- Followed repository test-comment structure and explicit assertion-message requirements.
- Did not revert or overwrite unrelated work; working tree only contains the Task 1 storage changes listed above.

## Review Fix: Missing-ID Update No Longer Fakes Success

The review finding on `AgentDefinitionStore::update()` is fixed.

### What changed

- Added a focused regression test for updating a missing id:
  - `test_agent_definition_store_update_missing_id_returns_typed_error`
- Added a typed storage error:
  - `StorageError::AgentDefinitionNotFound(i64)`
- Changed `AgentDefinitionStore::update()` so `changed == 0` returns `Err(StorageError::AgentDefinitionNotFound(input.id))` instead of a synthetic `AgentDefinition`.
- Kept the API shape stable:
  - `update()` still returns `StorageResult<AgentDefinition>`.
- Also aligned the create fallback to use the same typed not-found error if the inserted row cannot be reloaded.

### TDD Evidence

RED:

```bash
cargo test -p storage test_agent_definition_store_update_missing_id_returns_typed_error
```

Initial result before the fix:

- Exit code: `101`
- Failure reason: `StorageError::AgentDefinitionNotFound` did not exist yet, so the new regression test could not compile.

GREEN:

```bash
cargo test -p storage test_agent_definition_store_update_missing_id_returns_typed_error
cargo test -p storage agent_definition_store
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
```

Results:

- Missing-id regression test: passed
- `agent_definition_store` slice: `5 passed; 0 failed`
- `check-max-file-lines.sh`: passed
- `check-no-allow.sh`: passed
