# Project Agent Catalog Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add project-scoped Agent definitions that Settings can edit and Agent card creation can use.

**Architecture:** Store Agent definitions in each workspace project database as `agent_definitions`. The app loads definitions when a workspace opens, renders them in Settings → Agents, and creates Agent cards by copying definition name/profile into `AgentSpec.alias/profile`. Voice command routing is not implemented in this plan; only a pure helper prepares enabled Agent context for the future planner.

**Tech Stack:** Rust 2021, GPUI 0.2.2, rusqlite bundled SQLite, existing `storage`, `runtime`, `canvas`, and `app` crates.

## Global Constraints

- Agent definitions are project-scoped and must live in `<workspace>/.multisoul/project.sqlite`.
- Do not store project Agent definitions in app-level `settings`.
- Do not add `definition_id` to `AgentSpec` in this implementation.
- Do not send `workflow_description` to Agent CLI prompts in this implementation.
- Do not implement voice command routing in this implementation.
- Keep `storage` free of `app`, `runtime`, and `ui` dependencies.
- Keep Rust files at or below 500 lines.
- Do not add `#[allow(...)]`.
- New tests must include the repository-required scenario/data/process/expected-result comments and explicit assertion messages.

---

## File Structure

Create:

- `crates/storage/src/agent_definition_store.rs` — project-level CRUD for Agent definitions.
- `crates/storage/tests/unit/agent_definition_store_tests.rs` — storage tests.
- `crates/app/src/settings/agents.rs` — module facade, matching `settings/voice.rs`.
- `crates/app/src/settings/agents/state.rs` — settings state, validation, project-store binding, voice context helper.
- `crates/app/src/settings/agents/view.rs` — Settings → Agents GPUI content and handlers.
- `crates/app/tests/unit/agent_settings_tests.rs` — app-level state/view/card creation tests.

Modify:

- `crates/storage/src/migrations.rs` — create `agent_definitions` table and unique index.
- `crates/storage/src/project_store.rs` — add `agent_definition_store()`.
- `crates/storage/src/lib.rs` — export Agent definition types.
- `crates/app/src/app/mod.rs` — add `agent_definitions: AgentDefinitionsState`, initialize it.
- `crates/app/src/workspace/flow.rs` — bind/load project definitions on workspace open.
- `crates/app/src/settings/content.rs` — render `agents_settings(...)` for `SettingsSection::Agents`.
- `crates/app/src/settings/mod.rs` — expose `agents`.
- `crates/app/src/settings/panel.rs` — unhide Agents from sidebar if still hidden by comments.
- `crates/app/src/app/agent_actions.rs` — add a helper that creates an Agent card from `AgentDefinition`.

---

## Task 1: Storage Schema And Store

**Files:**
- Create: `crates/storage/src/agent_definition_store.rs`
- Create: `crates/storage/tests/unit/agent_definition_store_tests.rs`
- Modify: `crates/storage/src/migrations.rs`
- Modify: `crates/storage/src/project_store.rs`
- Modify: `crates/storage/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub struct AgentDefinition`
  - `pub struct NewAgentDefinition`
  - `pub struct AgentDefinitionUpdate`
  - `pub struct AgentDefinitionStore`
  - `impl AgentDefinitionStore::{new, create, list, update, delete}`
  - `ProjectStore::agent_definition_store(&self) -> AgentDefinitionStore`

- [ ] **Step 1: Write failing migration test**

Add this test to `crates/storage/tests/unit/agent_definition_store_tests.rs`:

```rust
use super::*;
use crate::migrations::open_project_connection;

/// Agent definitions migration: project DB 应创建 agent_definitions 表和大小写不敏感唯一索引。
///
/// 数据构造（含关键数值的推导过程）：
///   project.sqlite 位于 temp/workspace/.multisoul/project.sqlite
///   agent_definitions 需要 8 个业务列：id/name/runtime_profile/workflow_description/enabled/sort_order/created_at_unix/updated_at_unix
///   unique index = idx_agent_definitions_name_nocase，用于阻止 "Reviewer" 与 "reviewer" 同项目共存
///
/// 执行过程（逐步说明系统如何处理）：
///   1. open_project_connection(project.sqlite) 触发 migrate_project_db
///   2. PRAGMA table_info(agent_definitions) 读取列名
///   3. PRAGMA index_list(agent_definitions) 读取索引名
///
/// 预期结果：
///   - 正断言：agent_definitions 表包含 name/runtime_profile/workflow_description/enabled
///   - 正断言：存在 idx_agent_definitions_name_nocase
///   - 负断言：不应缺少 workflow_description，否则无法给未来语音规划提供上下文
#[test]
fn test_agent_definitions_migration_creates_table_and_unique_index() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let db_path = temp.path().join("workspace").join(".multisoul").join("project.sqlite");
    let connection = open_project_connection(&db_path).expect("project db should migrate");

    let mut columns_stmt = connection
        .prepare("PRAGMA table_info(agent_definitions)")
        .expect("table_info should prepare");
    let columns = columns_stmt
        .query_map([], |row| row.get::<_, String>(1))
        .expect("table_info should query")
        .collect::<Result<Vec<_>, _>>()
        .expect("columns should collect");

    let mut index_stmt = connection
        .prepare("PRAGMA index_list(agent_definitions)")
        .expect("index_list should prepare");
    let indexes = index_stmt
        .query_map([], |row| row.get::<_, String>(1))
        .expect("index_list should query")
        .collect::<Result<Vec<_>, _>>()
        .expect("indexes should collect");

    assert!(
        columns.iter().any(|column| column == "name"),
        "agent_definitions 必须包含 name 列作为 Settings 和语音匹配的显示名"
    );
    assert!(
        columns.iter().any(|column| column == "runtime_profile"),
        "agent_definitions 必须包含 runtime_profile 列以复用 Claude/Codex/OpenCode runtime"
    );
    assert!(
        columns.iter().any(|column| column == "workflow_description"),
        "agent_definitions 必须包含 workflow_description 列供未来语音 planner 使用"
    );
    assert!(
        columns.iter().any(|column| column == "enabled"),
        "agent_definitions 必须包含 enabled 列以便未来语音上下文过滤禁用 Agent"
    );
    assert!(
        indexes
            .iter()
            .any(|index| index == "idx_agent_definitions_name_nocase"),
        "agent_definitions 必须有大小写不敏感 name 唯一索引"
    );
    assert!(
        !columns.is_empty(),
        "agent_definitions 表不应缺失，否则 PRAGMA table_info 会返回空列表"
    );
}
```

- [ ] **Step 2: Run migration test and confirm RED**

Run:

```bash
cargo test -p storage test_agent_definitions_migration_creates_table_and_unique_index
```

Expected: FAIL because `agent_definitions` does not exist yet.

- [ ] **Step 3: Add migration**

In `migrate_project_db`, add this SQL inside the existing `execute_batch` block after `voice_history`:

```sql
CREATE TABLE IF NOT EXISTS agent_definitions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    runtime_profile TEXT NOT NULL,
    workflow_description TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at_unix INTEGER NOT NULL,
    updated_at_unix INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_definitions_name_nocase
ON agent_definitions(name COLLATE NOCASE);
```

- [ ] **Step 4: Run migration test and confirm GREEN**

Run:

```bash
cargo test -p storage test_agent_definitions_migration_creates_table_and_unique_index
```

Expected: PASS.

- [ ] **Step 5: Write failing CRUD tests**

Add tests to `agent_definition_store_tests.rs` for these names:

```rust
fn test_agent_definition_store_create_list_update_delete_round_trips()
fn test_agent_definition_store_rejects_case_insensitive_duplicate_names()
fn test_agent_definition_store_invalid_runtime_profile_returns_typed_error()
```

The round-trip test must create:

```rust
NewAgentDefinition {
    name: "Code Reviewer".to_string(),
    runtime_profile: AgentProfile::Codex,
    workflow_description: "Review code correctness, performance, and tests.".to_string(),
}
```

It must assert:

- created `id > 0`
- `list()` returns exactly one row
- update changes name/profile/workflow/enabled/sort_order
- delete returns `true`
- second delete returns `false`
- final list is empty

The duplicate test must create `"Reviewer"` and then attempt `"reviewer"` and assert the second call returns `Err`.

The invalid profile test must insert a row with `runtime_profile='bad-runtime'` using rusqlite and assert `list()` returns `StorageError::InvalidAgentProfile("bad-runtime".to_string())`.

- [ ] **Step 6: Run CRUD tests and confirm RED**

Run:

```bash
cargo test -p storage agent_definition_store
```

Expected: FAIL because `AgentDefinitionStore` is not implemented.

- [ ] **Step 7: Implement store**

Create `crates/storage/src/agent_definition_store.rs`:

```rust
use crate::{
    current_unix_timestamp, migrations::open_project_connection, StorageError, StorageResult,
};
use canvas::AgentProfile;
use rusqlite::{params, OptionalExtension};
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentDefinition {
    pub id: i64,
    pub name: String,
    pub runtime_profile: AgentProfile,
    pub workflow_description: String,
    pub enabled: bool,
    pub sort_order: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewAgentDefinition {
    pub name: String,
    pub runtime_profile: AgentProfile,
    pub workflow_description: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentDefinitionUpdate {
    pub id: i64,
    pub name: String,
    pub runtime_profile: AgentProfile,
    pub workflow_description: String,
    pub enabled: bool,
    pub sort_order: i64,
}

#[derive(Clone, Debug)]
pub struct AgentDefinitionStore {
    path: PathBuf,
}

impl AgentDefinitionStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn create(&self, input: &NewAgentDefinition) -> StorageResult<AgentDefinition> {
        let connection = open_project_connection(&self.path)?;
        let now = current_unix_timestamp()?;
        connection.execute(
            "INSERT INTO agent_definitions \
             (name, runtime_profile, workflow_description, enabled, sort_order, created_at_unix, updated_at_unix) \
             VALUES (?1, ?2, ?3, 1, 0, ?4, ?4)",
            params![
                input.name,
                encode_agent_profile(input.runtime_profile),
                input.workflow_description,
                now,
            ],
        )?;
        let id = connection.last_insert_rowid();
        self.get(id)?.ok_or(StorageError::InvalidAgentProfile(
            "missing-created-agent-definition".to_string(),
        ))
    }

    pub fn list(&self) -> StorageResult<Vec<AgentDefinition>> {
        let connection = open_project_connection(&self.path)?;
        let mut statement = connection.prepare(
            "SELECT id, name, runtime_profile, workflow_description, enabled, sort_order \
             FROM agent_definitions \
             ORDER BY sort_order ASC, name COLLATE NOCASE ASC",
        )?;
        let rows = statement.query_map([], decode_agent_definition_row)?;
        let mut definitions = Vec::new();
        for row in rows {
            let (id, name, runtime_profile, workflow_description, enabled, sort_order) = row?;
            definitions.push(AgentDefinition {
                id,
                name,
                runtime_profile: decode_agent_profile(&runtime_profile)?,
                workflow_description,
                enabled: enabled != 0,
                sort_order,
            });
        }
        Ok(definitions)
    }

    pub fn update(&self, input: &AgentDefinitionUpdate) -> StorageResult<AgentDefinition> {
        let connection = open_project_connection(&self.path)?;
        let changed = connection.execute(
            "UPDATE agent_definitions SET \
             name = ?1, runtime_profile = ?2, workflow_description = ?3, enabled = ?4, \
             sort_order = ?5, updated_at_unix = ?6 \
             WHERE id = ?7",
            params![
                input.name,
                encode_agent_profile(input.runtime_profile),
                input.workflow_description,
                if input.enabled { 1 } else { 0 },
                input.sort_order,
                current_unix_timestamp()?,
                input.id,
            ],
        )?;
        if changed == 0 {
            return Ok(AgentDefinition {
                id: input.id,
                name: input.name.clone(),
                runtime_profile: input.runtime_profile,
                workflow_description: input.workflow_description.clone(),
                enabled: input.enabled,
                sort_order: input.sort_order,
            });
        }
        self.get(input.id)?.ok_or(StorageError::InvalidAgentProfile(
            "missing-updated-agent-definition".to_string(),
        ))
    }

    pub fn delete(&self, id: i64) -> StorageResult<bool> {
        let connection = open_project_connection(&self.path)?;
        Ok(connection.execute("DELETE FROM agent_definitions WHERE id = ?1", params![id])? > 0)
    }

    fn get(&self, id: i64) -> StorageResult<Option<AgentDefinition>> {
        let connection = open_project_connection(&self.path)?;
        let row = connection
            .query_row(
                "SELECT id, name, runtime_profile, workflow_description, enabled, sort_order \
                 FROM agent_definitions WHERE id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            )
            .optional()?;
        match row {
            Some((id, name, runtime_profile, workflow_description, enabled, sort_order)) => {
                Ok(Some(AgentDefinition {
                    id,
                    name,
                    runtime_profile: decode_agent_profile(&runtime_profile)?,
                    workflow_description,
                    enabled: enabled != 0,
                    sort_order,
                }))
            }
            None => Ok(None),
        }
    }
}

fn decode_agent_definition_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<(
    i64,
    String,
    String,
    String,
    i64,
    i64,
)> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}

fn encode_agent_profile(profile: AgentProfile) -> &'static str {
    match profile {
        AgentProfile::Claude => "claude",
        AgentProfile::Codex => "codex",
        AgentProfile::OpenCode => "opencode",
    }
}

fn decode_agent_profile(profile: &str) -> StorageResult<AgentProfile> {
    match profile {
        "claude" => Ok(AgentProfile::Claude),
        "codex" => Ok(AgentProfile::Codex),
        "opencode" => Ok(AgentProfile::OpenCode),
        other => Err(StorageError::InvalidAgentProfile(other.to_string())),
    }
}

#[cfg(test)]
#[path = "../tests/unit/agent_definition_store_tests.rs"]
mod tests;
```

- [ ] **Step 8: Wire store exports**

In `crates/storage/src/lib.rs`, add:

```rust
pub mod agent_definition_store;
pub use agent_definition_store::{
    AgentDefinition, AgentDefinitionStore, AgentDefinitionUpdate, NewAgentDefinition,
};
```

In `crates/storage/src/project_store.rs`, import `AgentDefinitionStore` and add:

```rust
pub fn agent_definition_store(&self) -> AgentDefinitionStore {
    AgentDefinitionStore::new(self.path.clone())
}
```

- [ ] **Step 9: Run storage tests**

Run:

```bash
cargo test -p storage
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
```

Expected: all pass.

---

## Task 2: App State For Project Agent Definitions

**Files:**
- Create: `crates/app/src/settings/agents.rs`
- Create: `crates/app/src/settings/agents/state.rs`
- Create: `crates/app/tests/unit/agent_settings_tests.rs`
- Modify: `crates/app/src/settings/mod.rs`
- Modify: `crates/app/src/app/mod.rs`
- Modify: `crates/app/src/workspace/flow.rs`

**Interfaces:**
- Consumes:
  - `storage::AgentDefinition`
  - `storage::AgentDefinitionStore`
  - `storage::NewAgentDefinition`
  - `storage::AgentDefinitionUpdate`
- Produces:
  - `AgentDefinitionsState`
  - `AgentDefinitionDraft`
  - `AgentDefinitionField`
  - `AgentVoiceContext`
  - `AgentDefinitionsState::bind_project_store(store: AgentDefinitionStore)`
  - `AgentDefinitionsState::clear_project_store()`
  - `AgentDefinitionsState::voice_context() -> Vec<AgentVoiceContext>`

- [ ] **Step 1: Write failing state tests**

Create `crates/app/tests/unit/agent_settings_tests.rs` with tests:

```rust
fn test_agent_definitions_state_rejects_empty_name_and_workflow()
fn test_agent_definitions_state_loads_project_local_definitions()
fn test_agent_voice_context_includes_enabled_agents_only()
```

The validation test must assert:

- empty `name` returns error message `"Agent name is required"`
- empty `workflow_description` returns `"Workflow description is required"`
- over-80-char name returns `"Agent name must be 80 characters or fewer"`
- over-2000-char workflow returns `"Workflow description must be 2000 characters or fewer"`

The project-local test must create two temp workspaces, write one definition into each project DB, load them separately, and assert each state sees only its own project definition.

The voice-context test must create one enabled and one disabled definition, then assert only the enabled definition appears in `voice_context()`.

- [ ] **Step 2: Run state tests and confirm RED**

Run:

```bash
cargo test -p app agent_settings
```

Expected: FAIL because `settings::agents` does not exist.

- [ ] **Step 3: Create module facade**

Create `crates/app/src/settings/agents.rs`:

```rust
//! Project-scoped Agent catalog settings.

mod state;
mod view;

pub(crate) use state::{
    AgentDefinitionDraft, AgentDefinitionField, AgentDefinitionsState, AgentVoiceContext,
};
pub(crate) use view::agents_settings;

#[cfg(test)]
#[path = "../../tests/unit/agent_settings_tests.rs"]
mod tests;
```

Modify `crates/app/src/settings/mod.rs`:

```rust
pub(crate) mod agents;
```

- [ ] **Step 4: Implement state**

Create `crates/app/src/settings/agents/state.rs`:

```rust
use canvas::AgentProfile;
use storage::{
    AgentDefinition, AgentDefinitionStore, AgentDefinitionUpdate, NewAgentDefinition,
};

pub(crate) const MAX_AGENT_NAME_CHARS: usize = 80;
pub(crate) const MAX_WORKFLOW_DESCRIPTION_CHARS: usize = 2000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AgentDefinitionField {
    Name,
    WorkflowDescription,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentDefinitionDraft {
    pub(crate) id: Option<i64>,
    pub(crate) name: String,
    pub(crate) runtime_profile: AgentProfile,
    pub(crate) workflow_description: String,
    pub(crate) enabled: bool,
    pub(crate) sort_order: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentVoiceContext {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) runtime_profile: AgentProfile,
    pub(crate) workflow_description: String,
}

#[derive(Clone, Debug)]
pub(crate) struct AgentDefinitionsState {
    definitions: Vec<AgentDefinition>,
    store: Option<AgentDefinitionStore>,
    draft: AgentDefinitionDraft,
    status: Option<String>,
}

impl Default for AgentDefinitionsState {
    fn default() -> Self {
        Self {
            definitions: Vec::new(),
            store: None,
            draft: AgentDefinitionDraft::new(),
            status: None,
        }
    }
}

impl AgentDefinitionDraft {
    pub(crate) fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: String::new(),
            enabled: true,
            sort_order: 0,
        }
    }
}

impl AgentDefinitionsState {
    pub(crate) fn bind_project_store(&mut self, store: AgentDefinitionStore) {
        self.store = Some(store);
        self.reload();
    }

    pub(crate) fn clear_project_store(&mut self) {
        self.store = None;
        self.definitions.clear();
        self.draft = AgentDefinitionDraft::new();
        self.status = None;
    }

    pub(crate) fn definitions(&self) -> &[AgentDefinition] {
        &self.definitions
    }

    pub(crate) fn draft(&self) -> &AgentDefinitionDraft {
        &self.draft
    }

    pub(crate) fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub(crate) fn has_project_store(&self) -> bool {
        self.store.is_some()
    }

    pub(crate) fn set_draft_name(&mut self, value: impl Into<String>) {
        self.draft.name = value.into();
    }

    pub(crate) fn set_draft_workflow_description(&mut self, value: impl Into<String>) {
        self.draft.workflow_description = value.into();
    }

    pub(crate) fn set_draft_runtime_profile(&mut self, profile: AgentProfile) {
        self.draft.runtime_profile = profile;
    }

    pub(crate) fn validate_draft(&self) -> Result<(), String> {
        validate_agent_definition_text(&self.draft.name, &self.draft.workflow_description)
    }

    pub(crate) fn voice_context(&self) -> Vec<AgentVoiceContext> {
        self.definitions
            .iter()
            .filter(|definition| definition.enabled)
            .map(|definition| AgentVoiceContext {
                id: definition.id,
                name: definition.name.clone(),
                runtime_profile: definition.runtime_profile,
                workflow_description: definition.workflow_description.clone(),
            })
            .collect()
    }

    fn reload(&mut self) {
        let Some(store) = self.store.as_ref() else {
            self.definitions.clear();
            return;
        };
        match store.list() {
            Ok(definitions) => {
                self.definitions = definitions;
                self.status = None;
            }
            Err(error) => {
                self.definitions.clear();
                self.status = Some(format!("Could not load project agents: {error}"));
            }
        }
    }
}

pub(crate) fn validate_agent_definition_text(
    name: &str,
    workflow_description: &str,
) -> Result<(), String> {
    let name = name.trim();
    let workflow = workflow_description.trim();
    if name.is_empty() {
        return Err("Agent name is required".to_string());
    }
    if name.chars().count() > MAX_AGENT_NAME_CHARS {
        return Err("Agent name must be 80 characters or fewer".to_string());
    }
    if workflow.is_empty() {
        return Err("Workflow description is required".to_string());
    }
    if workflow.chars().count() > MAX_WORKFLOW_DESCRIPTION_CHARS {
        return Err("Workflow description must be 2000 characters or fewer".to_string());
    }
    Ok(())
}
```

Add create/update/delete methods after tests demand them in Task 3. Keep this task focused on load/validation/voice context.

- [ ] **Step 5: Add state to app and workspace flow**

In `crates/app/src/app/mod.rs`, import and add:

```rust
use crate::settings::agents::AgentDefinitionsState;

pub(crate) agent_definitions: AgentDefinitionsState,
```

Initialize:

```rust
agent_definitions: AgentDefinitionsState::default(),
```

In `enter_workspace_for_path`, after `project_store.migrate()` and before `persistence.set_project_store(project_store)`, bind definitions through an extra parameter or a follow-up app method. Prefer extending `enter_workspace_for_path` signature only if existing tests remain clear:

```rust
agent_definitions.bind_project_store(project_store.agent_definition_store());
```

Update all call sites and tests to pass `&mut agent_definitions`.

- [ ] **Step 6: Run app tests**

Run:

```bash
cargo test -p app agent_settings
cargo test -p app test_enter_workspace_records_recent_and_switches_to_canvas
cargo check -p app
```

Expected: all pass.

---

## Task 3: Settings Agents UI

**Files:**
- Create: `crates/app/src/settings/agents/view.rs`
- Modify: `crates/app/src/settings/content.rs`
- Modify: `crates/app/src/settings/panel.rs`
- Modify: `crates/app/tests/unit/agent_settings_tests.rs`

**Interfaces:**
- Consumes:
  - `AgentDefinitionsState`
  - `AgentDefinitionDraft`
- Produces:
  - `agents_settings(theme: Theme, state: &AgentDefinitionsState, cx: &mut Context<MultisoulApp>) -> Div`
  - click handlers on `MultisoulApp` for draft/profile/save/delete/toggle.

- [ ] **Step 1: Write failing view tests**

Add tests:

```rust
fn test_agents_settings_section_renders_real_content()
fn test_agents_settings_no_project_state_disables_create_controls()
fn test_agents_settings_content_routes_agents_section_to_agents_view()
```

Use `include_str!("../../src/settings/content.rs")` and `include_str!("../../src/settings/agents/view.rs")` source assertions where GPUI tree inspection is difficult.

Assert these exact ids appear:

- `settings-agents-list`
- `settings-agent-name-input`
- `settings-agent-workflow-input`
- `settings-agent-profile-codex`
- `settings-agent-save`
- `settings-agent-empty-project`

Also assert `SettingsSection::Agents => div()` is no longer present in `settings/content.rs`.

- [ ] **Step 2: Run view tests and confirm RED**

Run:

```bash
cargo test -p app agents_settings
```

Expected: FAIL because view is not wired.

- [ ] **Step 3: Implement view skeleton**

Create `crates/app/src/settings/agents/view.rs`:

```rust
use super::state::AgentDefinitionsState;
use crate::app::MultisoulApp;
use crate::settings::with_alpha;
use canvas::AgentProfile;
use gpui::{div, prelude::*, px, rgba, Context, Div, FontWeight};
use ui::Theme;

pub(crate) fn agents_settings(
    theme: Theme,
    state: &AgentDefinitionsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    if !state.has_project_store() {
        return div()
            .id("settings-agent-empty-project")
            .py(px(32.0))
            .text_color(rgba(theme.text_secondary))
            .child("Open a project to configure project Agents.");
    }

    div()
        .flex()
        .flex_col()
        .gap(px(18.0))
        .child(agent_form(theme, state, cx))
        .child(
            div()
                .id("settings-agents-list")
                .flex()
                .flex_col()
                .gap(px(10.0))
                .children(state.definitions().iter().map(|definition| {
                    div()
                        .id(("settings-agent-row", definition.id))
                        .p(px(12.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(rgba(with_alpha(theme.edge_highlight, 0x32)))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgba(theme.text_primary))
                                .child(definition.name.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgba(theme.text_secondary))
                                .child(definition.runtime_profile.display_title()),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgba(theme.text_tertiary))
                                .child(definition.workflow_description.clone()),
                        )
                })),
        )
}

fn agent_form(theme: Theme, state: &AgentDefinitionsState, cx: &mut Context<MultisoulApp>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(
            div()
                .id("settings-agent-name-input")
                .text_color(rgba(theme.text_secondary))
                .child(format!("Name: {}", state.draft().name)),
        )
        .child(profile_buttons(theme, state.draft().runtime_profile, cx))
        .child(
            div()
                .id("settings-agent-workflow-input")
                .text_color(rgba(theme.text_secondary))
                .child(format!(
                    "Workflow: {}",
                    state.draft().workflow_description
                )),
        )
        .child(
            div()
                .id("settings-agent-save")
                .cursor_pointer()
                .text_color(rgba(theme.text_primary))
                .child("Save Agent"),
        )
}

fn profile_buttons(theme: Theme, selected: AgentProfile, _cx: &mut Context<MultisoulApp>) -> Div {
    div()
        .flex()
        .flex_row()
        .gap(px(8.0))
        .child(profile_button(theme, "settings-agent-profile-claude", "Claude", selected == AgentProfile::Claude))
        .child(profile_button(theme, "settings-agent-profile-codex", "Codex", selected == AgentProfile::Codex))
        .child(profile_button(theme, "settings-agent-profile-opencode", "OpenCode", selected == AgentProfile::OpenCode))
}

fn profile_button(theme: Theme, id: &'static str, label: &'static str, selected: bool) -> Div {
    div()
        .id(id)
        .px(px(10.0))
        .py(px(6.0))
        .rounded(px(8.0))
        .bg(rgba(with_alpha(
            if selected { theme.accent } else { theme.node_fill },
            0xd0,
        )))
        .text_color(rgba(theme.text_primary))
        .child(label)
}
```

This skeleton may be refined in implementation to use actual editable controls consistent with existing Settings patterns. Keep ids stable for tests.

- [ ] **Step 4: Wire content routing**

In `crates/app/src/settings/content.rs`:

```rust
use crate::settings::agents::{agents_settings, AgentDefinitionsState};
```

Add `agent_definitions: &AgentDefinitionsState` parameter to `settings_content`.

Route:

```rust
SettingsSection::Agents => agents_settings(theme, agent_definitions, cx),
```

Pass `&self.agent_definitions` from `render_settings_panel`.

- [ ] **Step 5: Unhide Agents in sidebar**

In `settings/panel.rs`, ensure the rendered sidebar includes:

```rust
.child(section_button(theme, SettingsSection::Agents, active, cx))
```

Do not unhide mock-only `General`, `Runtime`, or `Skills` unless already present.

- [ ] **Step 6: Run app UI tests**

Run:

```bash
cargo test -p app agents_settings
cargo check -p app
```

Expected: all pass.

---

## Task 4: Create Agent Cards From Definitions

**Files:**
- Modify: `crates/app/src/app/agent_actions.rs`
- Modify: `crates/app/tests/unit/agent_settings_tests.rs`

**Interfaces:**
- Consumes:
  - `storage::AgentDefinition`
- Produces:
  - `agent_spec_from_definition(definition: &AgentDefinition) -> canvas::AgentSpec`
  - `MultisoulApp::spawn_agent_card_from_definition(definition_id: i64, window, cx)`

- [ ] **Step 1: Write failing pure mapping test**

Add:

```rust
fn test_agent_spec_from_definition_maps_name_to_alias_and_profile()
```

The test creates:

```rust
AgentDefinition {
    id: 7,
    name: "Code Reviewer".to_string(),
    runtime_profile: AgentProfile::Codex,
    workflow_description: "Review code.".to_string(),
    enabled: true,
    sort_order: 0,
}
```

Assert:

- `spec.profile == AgentProfile::Codex`
- `spec.alias.as_deref() == Some("Code Reviewer")`
- `spec.alias.as_deref() != Some("Codex")`

- [ ] **Step 2: Run mapping test and confirm RED**

Run:

```bash
cargo test -p app test_agent_spec_from_definition_maps_name_to_alias_and_profile
```

Expected: FAIL because helper does not exist.

- [ ] **Step 3: Implement mapping helper**

In `crates/app/src/app/agent_actions.rs`, add:

```rust
pub(crate) fn agent_spec_from_definition(
    definition: &storage::AgentDefinition,
) -> canvas::AgentSpec {
    canvas::AgentSpec {
        profile: definition.runtime_profile,
        alias: Some(definition.name.clone()),
    }
}
```

- [ ] **Step 4: Add spawn helper**

Add a method that finds by id and uses existing spawn path:

```rust
pub(crate) fn spawn_agent_card_from_definition(
    &mut self,
    definition_id: i64,
    window: &mut Window,
    cx: &mut Context<Self>,
) {
    let Some(definition) = self
        .agent_definitions
        .definitions()
        .iter()
        .find(|definition| definition.id == definition_id)
        .cloned()
    else {
        self.workspace_state.error = Some("Agent definition was not found.".to_string());
        cx.notify();
        return;
    };
    self.spawn_agent_card_with_alias(
        definition.runtime_profile,
        Some(definition.name),
        window,
        cx,
    );
}
```

Refactor existing `spawn_agent_card(profile, ...)` to call:

```rust
fn spawn_agent_card_with_alias(
    &mut self,
    profile: AgentProfile,
    alias: Option<String>,
    window: &mut Window,
    cx: &mut Context<Self>,
)
```

In this helper, call a new runtime helper:

```rust
Runtime::spawn_agent_with_alias(profile, alias, bounds, cols, rows)
```

Add `Runtime::spawn_agent_with_alias(profile, alias, bounds, cols, rows)` and keep existing `spawn_agent` delegating to it with `alias=None`.

- [ ] **Step 5: Add runtime helper tests**

Add a runtime test proving `spawn_agent_with_alias(..., Some("Code Reviewer"))` creates a document node with `alias=Some("Code Reviewer")`, while fixed `spawn_agent` keeps `alias=None`.

Run:

```bash
cargo test -p runtime spawn_agent
```

Expected: PASS.

- [ ] **Step 6: Run app tests**

Run:

```bash
cargo test -p app test_agent_spec_from_definition_maps_name_to_alias_and_profile
cargo test -p app
cargo check -p app
```

Expected: all pass.

---

## Task 5: Settings Save/Edit/Delete Actions

**Files:**
- Modify: `crates/app/src/settings/agents/state.rs`
- Modify: `crates/app/src/settings/agents/view.rs`
- Modify: `crates/app/tests/unit/agent_settings_tests.rs`

**Interfaces:**
- Consumes:
  - `AgentDefinitionsState`
  - `AgentDefinitionStore`
- Produces:
  - `AgentDefinitionsState::save_draft() -> bool`
  - `AgentDefinitionsState::edit_definition(id: i64)`
  - `AgentDefinitionsState::delete_definition(id: i64) -> bool`
  - `AgentDefinitionsState::toggle_enabled(id: i64) -> bool`

- [ ] **Step 1: Write failing state action tests**

Add tests:

```rust
fn test_agent_definitions_state_save_draft_creates_definition()
fn test_agent_definitions_state_edit_and_save_updates_definition()
fn test_agent_definitions_state_delete_removes_definition()
fn test_agent_definitions_state_duplicate_name_sets_readable_error()
```

Each test uses a temp project store. Assertions must check both in-memory state and persisted store list.

- [ ] **Step 2: Run action tests and confirm RED**

Run:

```bash
cargo test -p app agent_definitions_state
```

Expected: FAIL because action methods do not exist.

- [ ] **Step 3: Implement state actions**

Add methods to `AgentDefinitionsState`:

```rust
pub(crate) fn save_draft(&mut self) -> bool {
    if let Err(message) =
        validate_agent_definition_text(&self.draft.name, &self.draft.workflow_description)
    {
        self.status = Some(message);
        return false;
    }
    let Some(store) = self.store.as_ref() else {
        self.status = Some("Open a project before editing Agents.".to_string());
        return false;
    };

    let result = match self.draft.id {
        Some(id) => store.update(&AgentDefinitionUpdate {
            id,
            name: self.draft.name.trim().to_string(),
            runtime_profile: self.draft.runtime_profile,
            workflow_description: self.draft.workflow_description.trim().to_string(),
            enabled: self.draft.enabled,
            sort_order: self.draft.sort_order,
        }).map(|_| ()),
        None => store.create(&NewAgentDefinition {
            name: self.draft.name.trim().to_string(),
            runtime_profile: self.draft.runtime_profile,
            workflow_description: self.draft.workflow_description.trim().to_string(),
        }).map(|_| ()),
    };

    match result {
        Ok(()) => {
            self.draft = AgentDefinitionDraft::new();
            self.reload();
            true
        }
        Err(error) => {
            self.status = Some(readable_agent_store_error(&error));
            false
        }
    }
}
```

Implement `readable_agent_store_error`:

```rust
fn readable_agent_store_error(error: &storage::StorageError) -> String {
    let message = error.to_string();
    if message.contains("idx_agent_definitions_name_nocase")
        || message.contains("UNIQUE constraint failed")
    {
        "Agent name already exists in this project".to_string()
    } else {
        format!("Could not save project agent: {error}")
    }
}
```

- [ ] **Step 4: Wire GPUI handlers**

In `view.rs`, add click handlers:

- save button calls `app.agent_definitions.save_draft(); cx.notify();`
- profile buttons call `set_draft_runtime_profile`
- delete/toggle/edit buttons call corresponding state methods

Use the existing Voice Settings row edit pattern for text editing: clicking an edit control calls `begin_edit(field)`, `AgentDefinitionsState::handle_key_down(&Keystroke)` appends printable input to `draft`, `enter` commits the current field, and `escape` cancels editing.

- [ ] **Step 5: Run action tests**

Run:

```bash
cargo test -p app agent_definitions_state
cargo check -p app
```

Expected: all pass.

---

## Task 6: Verification And Documentation

**Files:**
- Modify: `docs/references/crate-file-map.md` only if new storage/app files need reference updates.
- No production code required unless earlier tasks expose doc gaps.

**Interfaces:**
- Consumes all previous tasks.
- Produces verified implementation.

- [ ] **Step 1: Run storage tests**

Run:

```bash
cargo test -p storage
```

Expected: PASS.

- [ ] **Step 2: Run runtime tests**

Run:

```bash
cargo test -p runtime
```

Expected: PASS.

- [ ] **Step 3: Run app tests**

Run:

```bash
cargo test -p app
```

Expected: PASS.

- [ ] **Step 4: Run app check**

Run:

```bash
cargo check -p app
```

Expected: PASS.

- [ ] **Step 5: Run formatting and constraints**

Run:

```bash
cargo fmt --check
bash scripts/check-storage-boundaries.sh
bash scripts/check-performance-guardrails.sh
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
bash scripts/check-agent-guide-lines.sh
```

Expected: all exit 0.

- [ ] **Step 6: Manual behavior check**

Run:

```bash
cargo run -p app
```

Manual expected behavior:

- Open a project.
- Open Settings → Agents.
- Add `Code Reviewer`, profile `Codex`, workflow description `Review code correctness, performance, and tests.`
- Create an Agent card from that definition.
- Card title shows `Code Reviewer`.
- Existing fixed Claude/Codex/OpenCode buttons still create cards.

- [ ] **Step 7: Commit**

Only after all checks pass:

```bash
git add \
  crates/storage/src/agent_definition_store.rs \
  crates/storage/tests/unit/agent_definition_store_tests.rs \
  crates/storage/src/migrations.rs \
  crates/storage/src/project_store.rs \
  crates/storage/src/lib.rs \
  crates/app/src/settings/agents.rs \
  crates/app/src/settings/agents/state.rs \
  crates/app/src/settings/agents/view.rs \
  crates/app/tests/unit/agent_settings_tests.rs \
  crates/app/src/settings/mod.rs \
  crates/app/src/app/mod.rs \
  crates/app/src/workspace/flow.rs \
  crates/app/src/settings/content.rs \
  crates/app/src/settings/panel.rs \
  crates/app/src/app/agent_actions.rs \
  docs/superpowers/specs/2026-07-01-project-agent-catalog-design.md \
  docs/superpowers/plans/2026-07-01-project-agent-catalog.md
git commit -m "feat(settings): add project agent catalog"
```
