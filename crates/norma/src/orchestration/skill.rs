use super::model::OrchestrationProvider;
use std::path::Path;

pub(crate) const MANAGED_MARKER: &str = "norma-orchestration-managed";

pub(crate) fn skill_source(
    provider: OrchestrationProvider,
    version: &str,
    cli_path: &Path,
) -> String {
    let command = cli_path.display();
    let body = format!(
        r#"# {MANAGED_MARKER}
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

pub(crate) fn managed_version(source: &str) -> Option<&str> {
    if !source.contains(MANAGED_MARKER) {
        return None;
    }
    source.lines().find_map(|line| {
        line.trim()
            .strip_prefix("# managed-version:")
            .map(str::trim)
    })
}
