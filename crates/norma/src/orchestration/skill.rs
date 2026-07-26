use super::model::OrchestrationProvider;
use std::path::Path;

pub(crate) const ORCHESTRATION_MANAGED_MARKER: &str = "norma-orchestration-managed";
pub(crate) const PRESENTATION_MANAGED_MARKER: &str = "norma-presentation-managed";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManagedSkillKind {
    Orchestration,
    Presentation,
}

pub(crate) fn skill_source(
    kind: ManagedSkillKind,
    provider: OrchestrationProvider,
    version: &str,
    cli_path: &Path,
) -> String {
    match kind {
        ManagedSkillKind::Orchestration => orchestration_skill_source(provider, version, cli_path),
        ManagedSkillKind::Presentation => presentation_skill_source(provider, version, cli_path),
    }
}

pub(crate) fn managed_marker(kind: ManagedSkillKind) -> &'static str {
    match kind {
        ManagedSkillKind::Orchestration => ORCHESTRATION_MANAGED_MARKER,
        ManagedSkillKind::Presentation => PRESENTATION_MANAGED_MARKER,
    }
}

fn orchestration_skill_source(
    provider: OrchestrationProvider,
    version: &str,
    cli_path: &Path,
) -> String {
    let command = cli_path.display();
    let body = format!(
        r#"# {ORCHESTRATION_MANAGED_MARKER}
# managed-version: {version}

# Norma OS Agent Orchestration

Use this capability only when the user explicitly asks you to delegate a specific task to a
specifically named Norma OS Agent. Never choose a target, split work, or delegate merely to add
parallelism without that explicit instruction.

1. Discover exact targets with `"{command}" agent list`.
2. Assign exactly one task with `"{command}" agent task assign <agent> --task <text>`.
3. Use the returned `task_id` with `status`, `wait`, or `cancel`.

Do not create or enable unknown Agents. Do not use `--force-untracked` unless the user explicitly
accepts that the target has no trustworthy completion tracking and explicitly authorizes
overwriting its input.
"#
    );
    match provider {
        OrchestrationProvider::Cursor => format!(
            "---\ndescription: Delegate explicitly authorized tasks through Norma OS\nalwaysApply: true\n---\n{body}"
        ),
        _ => format!(
            "---\nname: norma-orchestration\ndescription: Delegate explicitly authorized tasks through Norma OS\n---\n{body}"
        ),
    }
}

fn presentation_skill_source(
    provider: OrchestrationProvider,
    version: &str,
    cli_path: &Path,
) -> String {
    let command = cli_path.display();
    let body = format!(
        r#"# {PRESENTATION_MANAGED_MARKER}
# managed-version: {version}

# Norma OS Content Presentation

Use this capability to show Browser or read-only Preview cards in Norma OS:

- When the user explicitly asks to open, show, view, or preview content, you must present it.
- You may proactively present a generated deliverable or key evidence when seeing it materially helps.
- Present at most 3 items per user turn, serially and in the user's order. If there are more than
  3, present the first 3 and report which remaining items were not shown.
- Reuse an existing exact target by default. Use `--new` only when the user explicitly asks for
  another copy or a new card.

Commands:

1. Browser syntax: `norma present browser [url]`; run `"{command}" present browser [url]`.
2. Preview syntax: `norma present preview <path> [--type markdown|text|image]`; run
   `"{command}" present preview <path> [--type markdown|text|image]`.

Preview is strictly read-only and may show any readable file path explicitly supplied by the user;
it never modifies the source file. When you proactively select a file outside the current project,
confirm with the user before presenting it. Browser rejects `data:` and `javascript:` URLs.

For multiple items, keep successful earlier presentations and continue after deterministic
failures. If a command's side-effect outcome is unknown, stop immediately and do not retry it.
"#
    );
    match provider {
        OrchestrationProvider::Cursor => format!(
            "---\ndescription: Present Browser and read-only Preview cards through Norma OS\nalwaysApply: true\n---\n{body}"
        ),
        _ => format!(
            "---\nname: norma-presentation\ndescription: Present Browser and read-only Preview cards through Norma OS\n---\n{body}"
        ),
    }
}

pub(crate) fn managed_version(source: &str, kind: ManagedSkillKind) -> Option<&str> {
    if !source.contains(managed_marker(kind)) {
        return None;
    }
    source.lines().find_map(|line| {
        line.trim()
            .strip_prefix("# managed-version:")
            .map(str::trim)
    })
}
