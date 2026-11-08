//! Voice-command semantics. The host supplies the current scope; this module has no IO.

use crate::AgentRosterEntry;
mod cancellation;
mod constraints;
pub(crate) use constraints::{execution_is_paused, requires_action_receipt};
mod target;
use target::leading_members;
pub use target::{resolve_voice_target, VoiceTargetResolution};
mod clarification;
pub(crate) use clarification::{
    clarification_choices, is_clarification_attempt, target_only_answer,
    validate_clarified_proposals,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum VoiceFeedback {
    NewTaskForbidden,
    TargetMismatch,
    InvalidProposal,
    NoActionReceipt,
    ClarifyTarget {
        candidates: Vec<String>,
    },
    MissingSession,
    ScopeChanged,
    Submission {
        completed: usize,
        total: usize,
        uncertain: bool,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct VoiceTargetBinding {
    pub definition_id: i64,
    pub node_id: Option<u64>,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct VoiceCommit {
    pub(crate) proposal: VoiceProposal,
    pub binding: Option<VoiceTargetBinding>,
    input: String,
    original_input: String,
    requires_session: bool,
}

impl VoiceCommit {
    pub fn proposal(&self) -> &VoiceProposal {
        &self.proposal
    }

    pub(crate) fn matches(&self, tool: &str, arguments: &Value, input: &str) -> bool {
        self.proposal.tool == tool && self.proposal.arguments == *arguments && self.input == input
    }
}

pub fn revalidate_voice_commit(
    commit: &VoiceCommit,
    roster: &[AgentRosterEntry],
) -> Result<(), VoiceFeedback> {
    if let Some(binding) = &commit.binding {
        let targets: Vec<_> = roster
            .iter()
            .filter(|member| {
                member.enabled
                    && member.definition_id == binding.definition_id
                    && member.active_node_id == binding.node_id
            })
            .collect();
        let [target] = targets.as_slice() else {
            return Err(VoiceFeedback::TargetMismatch);
        };
        if commit.requires_session
            && (target.task_state == crate::AgentTaskState::Ended
                || target.active_node_id.is_none())
        {
            return Err(VoiceFeedback::MissingSession);
        }
        if target.active_node_id != binding.node_id || target.name != binding.name {
            return Err(VoiceFeedback::TargetMismatch);
        }
    }
    Ok(())
}

pub fn validate_voice_commit_scope(
    context: &crate::ToolContext,
    current_project: &str,
    current_generation: u64,
    commit: &VoiceCommit,
    roster: &[AgentRosterEntry],
) -> Result<(), VoiceFeedback> {
    if context.cancellation.is_cancelled()
        || context.project_id != current_project
        || context.user_input != commit.input
        || context.input_source
            != (crate::AgentInputSource::SingleVoiceCommand {
                generation: current_generation,
            })
    {
        return Err(VoiceFeedback::ScopeChanged);
    }
    revalidate_voice_commit(commit, roster)
}

pub fn validate_voice_proposals(
    input: &str,
    roster: &[AgentRosterEntry],
    proposals: Vec<VoiceProposal>,
) -> Result<Vec<VoiceCommit>, VoiceFeedback> {
    validate_against_target(
        input,
        roster,
        proposals,
        resolve_voice_target(input, roster),
    )
}

fn validate_against_target(
    input: &str,
    roster: &[AgentRosterEntry],
    proposals: Vec<VoiceProposal>,
    expected: VoiceTargetResolution,
) -> Result<Vec<VoiceCommit>, VoiceFeedback> {
    let commits: Result<Vec<_>, _> = proposals
        .into_iter()
        .map(|mut proposal| {
            let command =
                crate::adapter::parse_adapter_command(&proposal.tool, proposal.arguments.clone())
                    .map_err(|_| VoiceFeedback::InvalidProposal)?;
            if (forbids_new_task(input)
                || ["是什么意思", "什么意思"]
                    .iter()
                    .any(|marker| input.contains(marker)))
                && matches!(
                    command,
                    crate::AdapterCommand::AssignAgentTask { .. }
                        | crate::AdapterCommand::CreateAgent { .. }
                        | crate::AdapterCommand::AssignWorkspaceTask { .. }
                        | crate::AdapterCommand::HandoffToBrain { .. }
                        | crate::AdapterCommand::HandoffWithContext { .. }
                )
            {
                return Err(VoiceFeedback::NewTaskForbidden);
            }
            if matches!(
                command,
                crate::AdapterCommand::AssignIdea { .. }
                    | crate::AdapterCommand::AssignWorkspaceTask { .. }
                    | crate::AdapterCommand::WorkspaceMemory { .. }
                    | crate::AdapterCommand::OpenWorkspaceArtifact { .. }
                    | crate::AdapterCommand::LocateWorkspaceAgent { .. }
                    | crate::AdapterCommand::DeleteCanvasCardAtPosition { .. }
            ) {
                return Err(VoiceFeedback::InvalidProposal);
            }
            if matches!(
                expected,
                VoiceTargetResolution::CanvasAgent { .. }
                    | VoiceTargetResolution::OutsideCanvas { .. }
            ) && !is_read_only_tool(&proposal.tool)
                && !matches!(
                    command,
                    crate::AdapterCommand::AssignAgentTask { .. }
                        | crate::AdapterCommand::ReplyAgent { .. }
                        | crate::AdapterCommand::DeleteAgentCard { .. }
                        | crate::AdapterCommand::CancelAgentTask { .. }
                        | crate::AdapterCommand::CancelAgentReply { .. }
                )
            {
                return Err(VoiceFeedback::TargetMismatch);
            }
            if !is_read_only_tool(&proposal.tool) && execution_is_paused(input) {
                return Err(VoiceFeedback::InvalidProposal);
            }
            if !is_read_only_tool(&proposal.tool)
                && (expected == VoiceTargetResolution::Ambiguous
                    || (expected == VoiceTargetResolution::Unspecified
                        && matches!(
                            command,
                            crate::AdapterCommand::HandoffToBrain { .. }
                                | crate::AdapterCommand::HandoffWithContext { .. }
                        )))
            {
                return Err(VoiceFeedback::ClarifyTarget {
                    candidates: clarification_choices(roster),
                });
            }
            if let crate::AdapterCommand::CreateAgent { profile, .. } = &command {
                let lower = input.to_lowercase();
                let creation = ["创建", "新建", "create ", "new "]
                    .iter()
                    .any(|marker| lower.contains(marker));
                let profile_id = serde_json::to_value(profile)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .unwrap_or_default();
                let named_type = !profile_id.is_empty()
                    && lower
                        .split(|c: char| !c.is_ascii_alphanumeric())
                        .any(|word| word == profile_id);
                let generic_agent = lower.contains("智能体")
                    || lower
                        .split(|c: char| !c.is_ascii_alphanumeric())
                        .any(|word| word == "agent");
                if !creation || (!named_type && !generic_agent) {
                    return Err(VoiceFeedback::ClarifyTarget {
                        candidates: clarification_choices(roster),
                    });
                }
            }
            let binding = match &command {
                crate::AdapterCommand::CancelAgentTask { .. }
                | crate::AdapterCommand::CancelAgentReply { .. } => {
                    Some(cancellation::binding(input, &expected, roster)?)
                }

                crate::AdapterCommand::AssignAgentTask { agent, .. }
                | crate::AdapterCommand::ReplyAgent { agent, .. }
                | crate::AdapterCommand::DeleteAgentCard { agent } => {
                    let targets: Vec<_> = roster
                        .iter()
                        .filter(|target| {
                            let expected_node_matches = match expected {
                                VoiceTargetResolution::CanvasAgent {
                                    definition_id,
                                    node_id,
                                } => {
                                    target.definition_id == definition_id
                                        && target.active_node_id == Some(node_id)
                                }
                                _ => true,
                            };
                            expected_node_matches
                                && target.enabled
                                && (target.name.eq_ignore_ascii_case(agent)
                                    || target
                                        .speech_alias
                                        .as_deref()
                                        .is_some_and(|alias| alias.eq_ignore_ascii_case(agent)))
                        })
                        .collect();
                    let [target] = targets.as_slice() else {
                        return Err(VoiceFeedback::TargetMismatch);
                    };
                    if matches!(
                        expected,
                        VoiceTargetResolution::Unspecified | VoiceTargetResolution::Ambiguous
                    ) {
                        let candidates = clarification_choices(roster);
                        return Err(VoiceFeedback::ClarifyTarget { candidates });
                    }
                    if expected == VoiceTargetResolution::Assistant {
                        return Err(VoiceFeedback::TargetMismatch);
                    }
                    if let VoiceTargetResolution::CanvasAgent {
                        definition_id,
                        node_id,
                    } = expected
                    {
                        if target.definition_id != definition_id
                            || target.active_node_id != Some(node_id)
                        {
                            return Err(VoiceFeedback::TargetMismatch);
                        }
                    }
                    if let VoiceTargetResolution::OutsideCanvas { definition_id } = expected {
                        if target.definition_id != definition_id {
                            return Err(VoiceFeedback::TargetMismatch);
                        }
                        if !["新任务", "新建", "创建", "new task", "create "]
                            .iter()
                            .any(|marker| input.to_lowercase().contains(marker))
                        {
                            return Err(VoiceFeedback::MissingSession);
                        }
                    }
                    if matches!(command, crate::AdapterCommand::ReplyAgent { .. })
                        || requires_existing_session(input)
                    {
                        if target.active_node_id.is_none()
                            || target.task_state == crate::AgentTaskState::Ended
                        {
                            return Err(VoiceFeedback::MissingSession);
                        }
                        if matches!(command, crate::AdapterCommand::AssignAgentTask { .. }) {
                            return Err(VoiceFeedback::NewTaskForbidden);
                        }
                    }
                    Some(VoiceTargetBinding {
                        definition_id: target.definition_id,
                        node_id: target.active_node_id,
                        name: target.name.clone(),
                    })
                }
                _ => None,
            };
            let requires_session = matches!(command, crate::AdapterCommand::ReplyAgent { .. })
                || requires_existing_session(input);
            let content_field = match command {
                crate::AdapterCommand::AssignAgentTask { .. }
                | crate::AdapterCommand::CreateAgent { task: Some(_), .. } => Some("task"),
                crate::AdapterCommand::ReplyAgent { .. } => Some("message"),
                _ => None,
            };
            if let Some(field) = content_field {
                proposal.arguments[field] = Value::String(input.to_owned());
                if let Some(binding) = &binding {
                    proposal.arguments["agent"] = Value::String(binding.name.clone());
                }
            }
            Ok(VoiceCommit {
                proposal,
                binding,
                input: input.to_owned(),
                original_input: input.to_owned(),
                requires_session,
            })
        })
        .collect();
    let mut unique = Vec::new();
    for commit in commits? {
        if !unique.contains(&commit) {
            unique.push(commit);
        }
    }
    Ok(unique)
}

pub(crate) fn is_read_only_tool(name: &str) -> bool {
    matches!(
        name,
        "memory_list"
            | "memory_search"
            | "memory_read"
            | "context_read"
            | "workspace_snapshot"
            | "task_list"
            | "idea_list"
    )
}

pub(crate) fn forbids_new_task(input: &str) -> bool {
    let input = input.to_lowercase().replace(char::is_whitespace, "");
    [
        "不要新建",
        "别新建",
        "不新建",
        "不要创建",
        "别创建",
        "不创建",
        "don'tcreate",
        "donotcreate",
        "nonewtask",
    ]
    .iter()
    .any(|marker| input.contains(marker))
}

fn requires_existing_session(input: &str) -> bool {
    let input = input.to_lowercase();
    ["继续", "刚才", "之前", "continue "]
        .iter()
        .any(|marker| input.contains(marker))
        && !["新建", "创建", "create "]
            .iter()
            .any(|marker| input.contains(marker))
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct VoiceProposal {
    pub tool: String,
    pub arguments: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VoicePlan {
    Proposals(Vec<VoiceProposal>),
    NeedsModel,
}

pub fn plan_voice_command(input: &str, roster: &[AgentRosterEntry]) -> VoicePlan {
    let lower = input.to_lowercase();
    let target = resolve_voice_target(input, roster);
    let other_member_mentioned = match target {
        VoiceTargetResolution::CanvasAgent { definition_id, .. } => roster
            .iter()
            .filter(|member| member.enabled && member.definition_id != definition_id)
            .any(|member| {
                std::iter::once(member.name.as_str())
                    .chain(member.speech_alias.as_deref())
                    .filter(|name| !name.trim().is_empty())
                    .any(|name| lower.contains(&name.to_lowercase()))
            }),
        _ => false,
    };
    let complex = [
        "不",
        "别",
        "未",
        "解释",
        "什么意思",
        "是否",
        "怎么",
        "?",
        "？",
        "\"",
        "'",
        "‘",
        "’",
        "“",
        "”",
        "`",
        "「",
        "」",
        " not ",
        "don't",
        "instead",
        "actually",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    let explicit_action = leading_members(input.trim_start(), roster)
        .first()
        .is_some_and(|(_, length)| {
            let mut body = input.trim_start()[*length..]
                .trim_start_matches(|c: char| c.is_whitespace() || "，,:：".contains(c));
            for prefix in ["请", "帮我", "麻烦"] {
                body = body.strip_prefix(prefix).unwrap_or(body).trim_start();
            }
            [
                "新建任务",
                "新任务",
                "创建任务",
                "继续",
                "修复",
                "检查",
                "实现",
                "测试",
                "重构",
                "整理",
                "回复",
                "补充",
                "继续",
                "continue ",
                "fix ",
                "check ",
                "new task",
            ]
            .iter()
            .any(|verb| body.to_lowercase().starts_with(verb))
        });
    let ended_continuation = requires_existing_session(input) && roster.iter().any(|member| {
        matches!(target, VoiceTargetResolution::CanvasAgent { definition_id, node_id } if member.definition_id == definition_id && member.active_node_id == Some(node_id))
            && member.task_state == crate::AgentTaskState::Ended
    });
    if !explicit_action
        || ended_continuation
        || complex
        || other_member_mentioned
        || !matches!(target, VoiceTargetResolution::CanvasAgent { .. })
    {
        return VoicePlan::NeedsModel;
    }
    match crate::plan_direct_address(input, roster) {
        crate::DirectPlan::Calls(calls) if calls.len() == 1 => VoicePlan::Proposals(
            calls
                .into_iter()
                .map(|call| VoiceProposal {
                    tool: call.tool.into(),
                    arguments: call.arguments,
                })
                .collect(),
        ),
        crate::DirectPlan::Calls(_) | crate::DirectPlan::Skip | crate::DirectPlan::Say(_) => {
            VoicePlan::NeedsModel
        }
    }
}
