use super::model::OrchestrationProvider;
use std::path::Path;

pub(crate) const ORCHESTRATION_MANAGED_MARKER: &str = "norma-orchestration-managed";
pub(crate) const PRESENTATION_MANAGED_MARKER: &str = "norma-presentation-managed";
const ORCHESTRATION_SKILL_DESCRIPTION: &str = concat!(
    "Use this skill whenever the user asks to assign/delegate/publish a task to a specifically ",
    "named Norma OS Agent. Examples include 给 Simon 发布一个任务 and 让 Simon 给浩辰发布一个任务. ",
    "Cover direct and nested/subtask delegation, including speech-to-text with extra spaces or ",
    "punctuation. Do not use for mere mentions or unsolicited delegation."
);

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

Treat assign, delegate, hand off, pass, publish, 发任务, 发布任务, 分配, 委派, 交给, and equivalent
wording as explicit delegation when both the target Agent and requested action are identifiable.
Ignore incidental spacing, casing, or punctuation from speech-to-text. A request may assign one
Agent a task whose text instructs that Agent to delegate a subtask to another named Agent; this is
still an explicit nested delegation. Assign only the first target yourself and preserve the nested
delegation instruction in that task text so the target Agent can perform the next assignment.

NormaServer is owned once by the Norma OS App process, not by a window.
Do not start one NormaServer per window. If the CLI reports `app_unavailable` or a protocol/version
error, collect real evidence with `"{command}" doctor --format json` and
`"{command}" --version`; do not infer that the current window should start another server.

Self-identity is Workspace-scoped: "self" means the same Workspace and definition, never a same
name in another Workspace. Before any same-Workspace named delegation, run
`"{command}" agent self --format json` to learn your current canonical name, persistent speech
alias, Workspace ID, and node ID. Re-query before every side effect; do not cache identity across
renames.

- If the user addressed you as "you"/"yourself"/"current Agent", treat the request as self and do
  not call task assign at all.
- Compare the delegation target against your own canonical name (case-insensitive for English) and
  your actual speech alias, tolerating the supported leading/trailing spaces and `,`，`:`：`。`!`！`.
- If the target is yourself, do not generate an operation_id and do not call task assign; treat the
  remaining task text as the current task, or tell the user the task cannot be re-delegated to you.
- The same name in a different Workspace is not yourself. If the explicitly targeted Workspace
  differs from your own Workspace ID, resolve it through the cross-Workspace flow.
- If the `agent self` query fails or your node is not present in the roster, stop dispatching and
  report that the source identity cannot be confirmed; only retry read-only diagnostics with
  bounded backoff.

Same-Workspace delegation:

1. Discover exact targets with `"{command}" agent list --format json`.
2. Generate one opaque UUID as `<operation_id>` and retain it for this assignment.
3. Assign exactly one task with `"{command}" agent task assign <agent> --task <text>
   --operation-id <operation_id> --format json`.

Cross-Workspace delegation:

1. Discover open Workspaces with `"{command}" workspace list --format json` and select the exact stable
   `<workspace_id>`. Never guess from a display name or filesystem basename.
2. Discover targets there with `"{command}" agent list --workspace <workspace_id> --format json`.
3. Generate one opaque UUID as `<operation_id>` and retain it for this assignment.
4. Assign exactly one task with `"{command}" agent task assign <agent> --task <text>
   --workspace <workspace_id> --operation-id <operation_id> --format json`.

Pass Agent names, task text, Workspace IDs, task refs, and operation IDs as separately quoted argv
values or structured tool arguments. Never concatenate untrusted user text into executable shell
syntax.

After assignment, accept success only when the CLI returns a task receipt. For a managed receipt,
automatically call `wait <task_id> --format json` when the user asked for completion. For an
untracked receipt, stop automatic waiting and explain that completion cannot be tracked reliably.
A terminal status is authoritative even when tracking is `untracked`. The terminal response includes the task result
at `result.task.result`. If a non-terminal untracked task cannot be waited on, report that limitation
rather than claiming completion.

For a one-shot result lookup, run `"{command}" agent task result <task_id> --format json`. Read only
the result bound to that exact opaque task ID. A `task_result_not_ready` error means the task exists
but has not reached a terminal state; it is retryable as a read-only query with bounded backoff.
When a terminal result reports `availability=unavailable`, preserve the terminal status and say that
the Provider did not supply a trustworthy final reply; never substitute the latest conversation.

If an assignment returns `outcome_unknown`, times out, disconnects, or otherwise has an uncertain
side-effect result, stop and diagnose first. To resolve the original assignment, retry only by
replaying the identical command and reuse the same operation_id. Never create a new operation ID
for that retry, because doing so can create a duplicate task.

Deterministic rejections are authoritative and never retryable. When `agent task assign` returns
`self_dispatch_forbidden`, the canonical target is you yourself in the same Workspace: stop
immediately, do not change the target spelling or Workspace, and do not generate a new
operation_id; execute the remaining task yourself or tell the user the task cannot be
re-delegated to you. `delegation_cycle` means the target is an ancestor already active in this
delegation chain; stop and do not retry. `delegation_budget_exceeded` means the root task already
has the maximum unfinished descendants; stop and do not retry. For any of these deterministic
codes, never retry by inventing a new operation_id.

Cancellation is also a mutating operation. Generate and retain one cancellation operation ID, then
run `"{command}" agent task cancel <task_id> --operation-id <operation_id> --format json`. If its
outcome is unknown, replay only that identical cancel command with the same cancellation operation
ID; do not generate a replacement ID.

For `orchestration_checking`, only retry read-only diagnostics or discovery with bounded backoff;
do not automatically replay an assignment. Stop on `workspace_not_open`, `ambiguous_workspace`,
`client_upgrade_required`, or `protocol_version_mismatch`. Do not open a Workspace, modify an
installation, or select another target as an automatic fallback.

Do not create or enable unknown Agents. Do not use `--force-untracked` unless the user explicitly
accepts that the target has no trustworthy completion tracking and explicitly authorizes
overwriting its input. Never automatically add that flag while diagnosing availability or routing.
"#
    );
    match provider {
        OrchestrationProvider::Cursor => format!(
            "---\ndescription: {ORCHESTRATION_SKILL_DESCRIPTION}\nalwaysApply: true\n---\n{body}"
        ),
        _ => format!(
            "---\nname: norma-orchestration\ndescription: {ORCHESTRATION_SKILL_DESCRIPTION}\n---\n{body}"
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

Use this capability to show Browser, external Ego Lite, or read-only Preview cards in Norma OS:

- When the user explicitly asks to open, show, view, or preview content, you must present it.
- You may proactively present a generated deliverable or key evidence when seeing it materially helps.
- Present at most 3 items per user turn, serially and in the user's order. If there are more than
  3, present the first 3 and report which remaining items were not shown.
- Reuse an existing exact target by default. Use `--new` only when the user explicitly asks for
  another copy or a new card.

Commands:

1. Browser syntax: `norma present browser [url]`; run `"{command}" present browser [url]`.
2. Ego syntax: `norma present ego [--new]`; run `"{command}" present ego [--new]`.
   Ego is one dedicated external Ego Lite window represented by a Canvas status card; it is not a
   native Browser surface and does not automatically become Master. Reuse the existing card by
   default. Use `--new` only when the user explicitly requests a new task and the prior task is
   completed or failed.
3. Preview syntax: `norma present preview <path> [--type markdown|text|image]`; run
   `"{command}" present preview <path> [--type markdown|text|image]`.

Ego lifecycle after `present ego` returns a `node_id`:

- After you have explicitly acquired the matching Ego Task Space, report work with
  `norma ego running <node_id>`.
- When manual user action is needed, first call `handOffTaskSpace(task_space_id)` in the Ego helper
  and verify `{{ done: true }}`, then run `norma ego handoff <node_id> --message <what-to-do>` and
  stop browser work. The user sets the card as Master and clicks Take over in Norma.
- Do not automatically call `takeOverTaskSpace`; only after the user clicks Return to AI in Norma
  may you call it, then report `norma ego running <node_id>` after it succeeds.
- When the task is finished, make one dedicated final Ego helper call to
  `completeTaskSpace(task_space_id, {{ keep: true }})` and verify `{{ done: true }}`. Never use
  `keep: false` while the Task Space is attached to an Ego card: Norma needs the retained page for
  live and final Canvas previews. Only then run `norma ego complete <node_id>`.
- For an unrecoverable failure, preserve the Task Space and run
  `norma ego fail <node_id> --message <reason>`. Use `norma ego status <node_id>` for a read-only
  status check.

Preview is strictly read-only and may show any readable file path explicitly supplied by the user;
it never modifies the source file. When you proactively select a file outside the current project,
confirm with the user before presenting it. Browser rejects `data:` and `javascript:` URLs.

For multiple items, keep successful earlier presentations and continue after deterministic
failures. If a command's side-effect outcome is unknown, stop immediately and do not retry it.
"#
    );
    match provider {
        OrchestrationProvider::Cursor => format!(
            "---\ndescription: Present Browser, external Ego Lite, and read-only Preview cards through Norma OS\nalwaysApply: true\n---\n{body}"
        ),
        _ => format!(
            "---\nname: norma-presentation\ndescription: Present Browser, external Ego Lite, and read-only Preview cards through Norma OS\n---\n{body}"
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
