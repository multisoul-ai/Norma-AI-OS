# Project Agent Catalog Design

## Goal

Add a project-scoped Agents settings module so each workspace can define reusable Agent templates with a name, runtime profile, and workflow description. Creating an Agent card from the project catalog must use these project definitions while preserving the existing Claude/Codex/OpenCode fixed-entry behavior.

This prepares the data model for future voice commands that route instructions to project-specific Agents.

## Decisions

- Agent definitions are scoped to the current project, not global app settings.
- Agent definitions live in `<workspace>/.multisoul/project.sqlite`.
- First implementation uses "copy on card creation":
  - `AgentDefinition.name` becomes `AgentSpec.alias`.
  - `AgentDefinition.runtime_profile` becomes `AgentSpec.profile`.
- `workflow_description` is stored for future voice planning context. It is not automatically sent to the Agent CLI in the first implementation.
- First implementation does not add `definition_id` to `AgentSpec`. The storage and app boundaries must keep `AgentDefinition.id` available so `definition_id: Option<i64>` can be added later without redesigning the catalog.

## Existing Storage Model

There are two SQLite scopes.

App-level database:

| Table | Responsibility |
|---|---|
| `recent_workspaces` | Recently opened projects |
| `settings` | Global app settings such as Voice, Wallpaper, and shortcuts |

Project-level database at `<workspace>/.multisoul/project.sqlite`:

| Table | Responsibility |
|---|---|
| `canvas_snapshots` | Current canvas document JSON, including node ids, bounds, and `AgentSpec` |
| `session_checkpoints` | Live session restore parameters by `node_id` |
| `node_sessions` | Runtime state such as terminal cwd and Agent CLI `session_id` by `node_id` |
| `layout_state` | Master/detail layout state |
| `voice_history` | Voice command audit history, including `target_node_id` |

Current project tables relate through application-managed `node_id` values:

```text
canvas_snapshots.json.nodes[].id
        |
        +--> session_checkpoints.node_id
        |
        +--> node_sessions.node_id
        |
        +--> voice_history.target_node_id
```

The new `agent_definitions` table is not keyed by `node_id`. It stores reusable templates, not live card instances.

## New Table

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

`runtime_profile` uses the same encoding style as `session_checkpoints.agent_profile`:

| Rust | SQLite |
|---|---|
| `AgentProfile::Claude` | `claude` |
| `AgentProfile::Codex` | `codex` |
| `AgentProfile::OpenCode` | `opencode` |

## Rust Data Model

In `storage`:

```rust
pub struct AgentDefinition {
    pub id: i64,
    pub name: String,
    pub runtime_profile: AgentProfile,
    pub workflow_description: String,
    pub enabled: bool,
    pub sort_order: i64,
}

pub struct NewAgentDefinition {
    pub name: String,
    pub runtime_profile: AgentProfile,
    pub workflow_description: String,
}

pub struct AgentDefinitionUpdate {
    pub id: i64,
    pub name: String,
    pub runtime_profile: AgentProfile,
    pub workflow_description: String,
    pub enabled: bool,
    pub sort_order: i64,
}
```

Add `AgentDefinitionStore` with these first-version operations:

```rust
impl AgentDefinitionStore {
    pub fn create(&self, input: &NewAgentDefinition) -> StorageResult<AgentDefinition>;
    pub fn list(&self) -> StorageResult<Vec<AgentDefinition>>;
    pub fn update(&self, input: &AgentDefinitionUpdate) -> StorageResult<AgentDefinition>;
    pub fn delete(&self, id: i64) -> StorageResult<bool>;
}
```

Add this factory:

```rust
impl ProjectStore {
    pub fn agent_definition_store(&self) -> AgentDefinitionStore;
}
```

## Validation

The app layer must validate before writing:

- `name.trim()` must not be empty.
- `name.trim()` must be at most 80 Unicode scalar values.
- `workflow_description.trim()` must not be empty.
- `workflow_description.trim()` must be at most 2000 Unicode scalar values.
- Duplicate names inside the same project are rejected case-insensitively by the database index and surfaced as a readable settings error.

`enabled=false` definitions remain stored but are hidden from future voice planning context by default.

## Settings UI

`SettingsSection::Agents` currently exists as a placeholder. It becomes the project Agent Catalog editor.

When no project is open:

- Show an empty state explaining that Agents are project-specific.
- Disable create/edit controls.

When a project is open:

- Load definitions from the current project store.
- Show a list of definitions ordered by `sort_order ASC, name COLLATE NOCASE ASC`.
- Each row shows:
  - name
  - runtime profile badge
  - workflow description summary
  - enabled/disabled state
- First implementation supports:
  - add
  - edit
  - delete
  - enable/disable

The UI follows the existing Settings split:

```text
crates/app/src/settings/agents.rs
crates/app/src/settings/agents/state.rs
crates/app/src/settings/agents/view.rs
```

This mirrors `settings/voice` and `settings/wallpaper`.

## Card Creation Flow

Existing fixed actions remain:

- New Claude Agent
- New Codex Agent
- New OpenCode Agent

New project definitions add a preferred creation path:

```text
AgentDefinition {
  name: "Code Reviewer",
  runtime_profile: AgentProfile::Codex,
  workflow_description: "Review code correctness, performance, tests..."
}
        |
        v
AgentSpec {
  profile: AgentProfile::Codex,
  alias: Some("Code Reviewer".to_string())
}
```

The created card uses existing runtime behavior:

- `Runtime::spawn_agent`
- `spawn_focus::activate_spawned_node`
- motion spawn
- current layout reflow
- `mark_canvas_dirty`
- `mark_session_dirty`

The workflow description is not sent to the spawned CLI in this version.

## Voice Preparation

Future voice planning can build a project-local Agent context from enabled definitions:

```text
Available agents in this project:
- Code Reviewer [Codex]: Review code correctness, performance, tests...
- Product Strategist [Claude]: Clarify requirements and product direction...
```

Future routing can use:

```rust
pub struct VoiceAgentTarget {
    pub agent_definition_id: Option<i64>,
    pub agent_name: String,
    pub message: String,
}
```

First-version matching can use card alias equality:

```text
Canvas Agent card alias == AgentDefinition.name
```

Later extension path: when templates need live synchronization, add `definition_id: Option<i64>` to `AgentSpec` in a migration-compatible change.

## Non-Goals

- Do not implement voice command routing in this feature.
- Do not add custom shell commands or arbitrary Agent runtime command templates.
- Do not automatically inject `workflow_description` into Agent CLI prompts.
- Do not automatically update existing cards when an Agent definition changes.
- Do not add cross-project/global Agent definitions.

## Testing Strategy

Storage tests:

- migration creates `agent_definitions` and the case-insensitive unique index.
- `create/list/update/delete` round trip.
- case-insensitive duplicate names fail.
- invalid `runtime_profile` decode fails with typed storage error.

App settings tests:

- `SettingsSection::Agents` renders real content instead of blank placeholder.
- no-project state disables editing.
- loaded project definitions appear in the Agents settings content.
- validation rejects empty name and empty workflow description.

Card creation tests:

- creating from an `AgentDefinition` maps `name` to `AgentSpec.alias`.
- creating from an `AgentDefinition` maps `runtime_profile` to `AgentSpec.profile`.
- fixed Claude/Codex/OpenCode creation still works.

Voice-prep tests:

- pure helper for voice context includes only enabled definitions.
- helper formats `name`, `runtime_profile`, and `workflow_description`.
